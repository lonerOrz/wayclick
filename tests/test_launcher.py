import os
import sys
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
            if cmd[0] == "pgrep":
                rc.returncode = 0 if running else 1
            elif cmd[0] == "notify-send":
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


if __name__ == "__main__":
    unittest.main()
