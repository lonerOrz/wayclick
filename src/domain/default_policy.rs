//! Policy for input events that have no explicit mapping.

/// What to do when an input event has no matching mapping rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DefaultPolicy {
    /// Pick a random default sound (duplicates in the defaults list are
    /// weighted). This is the historical behaviour and the default.
    #[default]
    Random,
    /// Ignore unmatched events entirely: no sound, no play request.
    Ignore,
}
