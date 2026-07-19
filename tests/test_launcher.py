import os
import sys
import importlib.util
import unittest
from unittest import mock


class TestLauncher(unittest.TestCase):
    def _run(
        self,
        system="linux",
        *,
        euid=1000,
        groups_out="user input",
        config_exists=True,
        running=False,
        notify_rc=0,
        spawn_rc=0,
    ):
        import wayclick

        wc = wayclick
        calls = []
        envs = {}

        def fake_run(cmd, **kw):
            calls.append(cmd)
            envs[cmd[0]] = kw.get("env")
            rc = mock.MagicMock()
            if cmd[0] == "notify-send":
                rc.returncode = notify_rc
            elif cmd[0] in ("python3", sys.executable, "python"):
                rc.returncode = spawn_rc
            else:
                rc.returncode = 0
            rc.stdout = mock.MagicMock()
            rc.stdout.decode.return_value = groups_out
            return rc

        with mock.patch("platform.system", return_value=system), mock.patch.object(
            wc.subprocess, "run", side_effect=fake_run
        ), mock.patch.object(wc.os, "geteuid", return_value=euid), mock.patch(
            "os.path.isfile", return_value=config_exists
        ), mock.patch.object(
            wc.ProcessManager, "is_running", return_value=running
        ):
            rc = wc.main()
        return rc, calls, envs

    def test_toggle_off_when_running(self):
        rc, calls, _ = self._run(running=True)
        self.assertEqual(rc, 0)
        self.assertIn(["pkill", "-f", "python.*runner_cross_platform"], calls)
        self.assertIn(["notify-send", "WayClick", "Disabled"], calls)

    def test_root_exits_1_on_linux(self):
        rc, calls, _ = self._run(euid=0)
        self.assertEqual(rc, 1)
        self.assertEqual(calls, [])  # never spawned

    def test_missing_input_group_exits_1_on_linux(self):
        rc, calls, _ = self._run(groups_out="user wheel")
        self.assertEqual(rc, 1)
        self.assertIn(["notify-send", "WayClick", "User not in input group"], calls)

    def test_missing_config_exits_1(self):
        rc, calls, _ = self._run(config_exists=False)
        self.assertEqual(rc, 1)
        self.assertIn(
            [
                "notify-send",
                "WayClick",
                f"Missing config file at {os.path.expanduser('~/.config/wayclick')}/config.json",
            ],
            calls,
        )

    def test_enabled_spawns_runner_on_linux(self):
        rc, calls, envs = self._run()
        self.assertEqual(rc, 0)
        spawn = [c for c in calls if "runner_cross_platform.py" in str(c)]
        self.assertEqual(len(spawn), 1)
        spawn_env = envs.get(spawn[0][0], {})
        self.assertEqual(spawn_env.get("ENABLE_TRACKPADS"), "false")
        self.assertTrue(any(c[0] in ("python3", "python") for c in spawn))

    def test_enabled_spawns_runner_on_darwin(self):
        rc, calls, _ = self._run(system="darwin")
        self.assertEqual(rc, 0)
        spawn = [c for c in calls if "runner_cross_platform.py" in str(c)]
        self.assertEqual(len(spawn), 1)
        # darwin config path differs
        self.assertIn("Library/Application Support/wayclick", spawn[0][-1])

    def test_notify_failure_is_silent(self):
        # notify-send failing must not crash the launcher
        rc, _, _ = self._run(notify_rc=1)
        self.assertEqual(rc, 0)


class TestPlatformPaths(unittest.TestCase):
    def test_single_source_of_truth(self):
        from platform_paths import config_dir
        import os

        self.assertEqual(
            config_dir("darwin"),
            os.path.expanduser("~/Library/Application Support/wayclick"),
        )
        self.assertEqual(config_dir("windows"), os.path.expanduser("~\\.wayclick"))
        self.assertEqual(config_dir("linux"), os.path.expanduser("~/.config/wayclick"))
        self.assertEqual(
            config_dir("freebsd"), os.path.expanduser("~/.config/wayclick")
        )


class TestProcessManager(unittest.TestCase):
    def _run(self, fn, pgrep_rc=1, pkill_rc=0):
        import process_manager

        calls = []

        def fake_run(cmd, **kw):
            calls.append(cmd)
            rc = mock.MagicMock()
            rc.returncode = pkill_rc if cmd[0] == "pkill" else pgrep_rc
            rc.stdout = mock.MagicMock()
            return rc

        with mock.patch.object(process_manager.subprocess, "run", side_effect=fake_run):
            result = fn(process_manager.ProcessManager())
        return result, calls

    def test_is_running_true_when_pgrep_match(self):
        result, calls = self._run(lambda pm: pm.is_running(), pgrep_rc=0)
        self.assertTrue(result)
        self.assertEqual(calls[0][:2], ["pgrep", "-f"])

    def test_is_running_false_when_no_match(self):
        result, _ = self._run(lambda pm: pm.is_running(), pgrep_rc=1)
        self.assertFalse(result)

    def test_is_running_false_when_pgrep_missing(self):
        import process_manager

        with mock.patch.object(
            process_manager.subprocess,
            "run",
            side_effect=FileNotFoundError,
        ):
            self.assertFalse(process_manager.ProcessManager().is_running())

    def test_toggle_off_sends_pkill(self):
        _, calls = self._run(lambda pm: pm.toggle_off())
        self.assertEqual(calls[0][:2], ["pkill", "-f"])

    def test_toggle_uses_injected_pattern(self):
        import process_manager

        pm = process_manager.ProcessManager(runner_pattern="custom.*pattern")
        self.assertEqual(pm.runner_pattern, "custom.*pattern")


class TestPyInstallerHook(unittest.TestCase):
    def _load_hook(self):
        # Stub PyInstaller (not installed in dev) and platform-specific deps so
        # the hook module loads and derives hidden imports from LISTENERS.
        hooks = mock.MagicMock()
        hooks.collect_submodules.return_value = []
        hooks.collect_dynamic_libs.return_value = []
        with mock.patch.dict(
            "sys.modules",
            {
                "PyInstaller": mock.MagicMock(),
                "PyInstaller.utils": mock.MagicMock(),
                "PyInstaller.utils.hooks": hooks,
                "evdev": mock.MagicMock(),
                "evdev.ecodes": mock.MagicMock(),
                "Quartz": mock.MagicMock(),
            },
        ):
            spec = importlib.util.spec_from_file_location(
                "hook_wayclick",
                os.path.join(
                    os.path.dirname(__file__),
                    "..",
                    "src",
                    "hook-runner_cross_platform.py",
                ),
            )
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
        return mod

    def test_hidden_imports_derive_from_registry(self):
        mod = self._load_hook()
        for name in ("input_handler", "linux_input", "windows_input", "macos_input"):
            self.assertIn(name, mod.hiddenimports)
        self.assertIn("ctypes", mod.hiddenimports)

    def test_hidden_imports_track_new_adapter(self):
        # If LISTENERS ever gains an adapter, the hook must pick it up
        # without a manual edit — proven by deriving from the live registry.
        with mock.patch.dict(
            "sys.modules",
            {
                "evdev": mock.MagicMock(),
                "evdev.ecodes": mock.MagicMock(),
                "Quartz": mock.MagicMock(),
            },
        ):
            import input_handler

            mod = self._load_hook()
            derived = {
                listener.__module__ for listener in input_handler.LISTENERS.values()
            }
        self.assertTrue(derived.issubset(set(mod.hiddenimports)))


if __name__ == "__main__":
    unittest.main()
