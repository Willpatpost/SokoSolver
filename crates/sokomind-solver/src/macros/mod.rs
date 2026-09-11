pub mod forced_push;
pub mod goal_macro;
pub mod tunnel;

use crate::compiled_board::CompiledBoard;

use self::goal_macro::SafeGoalMap;
use self::tunnel::TunnelMap;

/// Combined macro engine holding all precomputed macro data.
/// Built once per board during the preparation phase.
#[derive(Clone, Debug)]
pub struct MacroEngine {
    pub tunnels: TunnelMap,
    pub safe_goals: SafeGoalMap,
}

impl MacroEngine {
    pub fn new(cb: &CompiledBoard) -> Self {
        MacroEngine {
            tunnels: TunnelMap::analyze(cb),
            safe_goals: SafeGoalMap::analyze(cb),
        }
    }
}
