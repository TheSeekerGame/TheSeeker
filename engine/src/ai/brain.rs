//! Compiled FSM structures and brain logic.
//! 
//! Compilation flow:
//! 1. Parse TOML FSM → resolve state names to IDs
//! 2. Expand $CONSTANTS from archetype stats  
//! 3. Build action/condition lookups (slots, cooldowns)
//! 4. Output immutable CompiledFsm shared by all instances

use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::borrow::Cow;
use super::asset::{EnemyFsm, FsmState, FsmTransition, FsmCondition, FsmAction, ArchetypeStats, EnemyArchetype};

/// Type alias for state IDs  
pub type StateId = u16;

/// Compiled FSM ready for runtime execution
#[derive(Debug, Clone, TypePath, Asset)]
pub struct CompiledFsm {
    /// Internal compiled data
    pub inner: Arc<CompiledFsmInner>,
}

/// Inner compiled FSM data (immutable, shared via Arc)
#[derive(Debug)]
pub struct CompiledFsmInner {
    /// Starting state IDs
    pub start_logic: StateId,
    pub start_movement: StateId,
    
    /// State name to ID mappings
    pub logic_states: HashMap<String, StateId>,
    pub movement_states: HashMap<String, StateId>,
    
    /// ID to state name mappings (for debugging)
    pub logic_state_names: Vec<String>,
    pub movement_state_names: Vec<String>,
    
    /// Compiled rules for each track
    pub logic_rules: Vec<CompiledRule>,
    pub movement_rules: Vec<CompiledRule>,

    /// Actions associated with each logic state (indexed by StateId)
    pub logic_state_actions: Vec<StateActions>,
    /// Actions associated with each movement state (indexed by StateId)
    pub movement_state_actions: Vec<StateActions>,

    /// Lookup table for ScriptPlayer slot names -> index (0..31)
    pub slot_names: Vec<String>,

    /// Lookup table for cooldown names -> index
    pub cooldown_names: Vec<String>,
}

impl CompiledFsmInner {
    /// Return the slot id for `name`, if present.
    pub fn slot_id(&self, name: &str) -> Option<u8> {
        self.slot_names.iter().position(|s| s == name).map(|i| i as u8)
    }

    /// Return the cooldown id for `name`, if present.
    pub fn cooldown_id(&self, name: &str) -> Option<u8> {
        self.cooldown_names.iter().position(|s| s == name).map(|i| i as u8)
    }
}

/// Precompiled actions for a state
#[derive(Debug, Clone)]
pub struct StateActions {
    pub on_enter: Vec<CompiledAction>,
    pub tick: Vec<CompiledAction>,
}

impl StateActions {
    fn empty() -> Self {
        Self { on_enter: vec![], tick: vec![] }
    }
}

/// Compiled condition data (instead of function pointer)
#[derive(Debug, Clone)]
pub enum CompiledCondition {
    /// Always true
    Always,
    
    /// Distance less than value
    DistanceLt(f32),
    
    /// Distance greater than value
    DistanceGt(f32),
    
    /// Timer greater than value
    TimerGt(u16),
    
    /// Is grounded check
    IsGrounded(bool),
    
    /// Health is zero
    HealthZero,
    
    /// Random chance (0-255)
    RngChance(u8),

    /// Check a ScriptPlayer slot (by index) equals expected bool
    Slot { id: u8, expected: bool },

    /// Internal cooldown table entry ready (value == 0)
    CooldownReady(u8),
}

/// Compiled transition rule
#[derive(Debug)]
pub struct CompiledRule {
    /// Which states this rule can fire from
    pub from_pattern: FromPattern,
    
    /// Conditions to check (ALL must be true)
    pub conditions: Vec<CompiledCondition>,
    
    /// Actions to execute if rule fires
    pub actions: Vec<CompiledAction>,
    
    /// Target state (if transitioning)
    pub target_state: Option<StateId>,
    
    /// Priority (lower = higher priority)
    pub priority: u16,
    
    /// Stop evaluating further rules this tick if this fires
    pub terminal: bool,
}

/// Pattern for matching source states
#[derive(Debug)]
pub enum FromPattern {
    /// Specific single state
    Specific(StateId),
    /// Any state
    Any,
    /// Multiple specific states
    Multiple(Vec<StateId>),
}

impl FromPattern {
    /// Check if pattern matches current state
    pub fn matches(&self, current: StateId) -> bool {
        match self {
            FromPattern::Specific(id) => *id == current,
            FromPattern::Any => true,
            FromPattern::Multiple(ids) => ids.contains(&current),
        }
    }
}

/// Compiled action ready for execution
#[derive(Debug, Clone)]
pub enum CompiledAction {
    /// Play animation by key
    PlayAnim(Cow<'static, str>),
    
    /// Set velocity directly
    SetVel(Vec2),
    
    /// Set velocity towards player at given speed
    SetVelTowardsPlayer(f32),
    
    /// Set velocity based on facing direction at given speed
    SetVelFromFacing(f32),
    
    /// Face towards player
    FacePlayer,
    
    /// Spawn attack prefab
    SpawnAttack {
        key: String,
        dmg: f32,
    },
    
    /// Set cooldown timer
    Cooldown {
        name: String,
        ticks: u16,
    },

    /// Execute when animation reaches specific frame (repeats on loop)
    Delayed {
        tick: u16,
        inner: Box<CompiledAction>,
    },

    /// Execute once when state timer reaches tick (resets on state/anim change)
    StateDelayed {
        tick: u16,
        inner: Box<CompiledAction>,
    },
}

impl CompiledFsm {
    /// Collect all animation keys referenced by PlayAnim actions in this FSM
    pub fn collect_animation_keys(&self) -> Vec<String> {
        let mut animation_keys = Vec::new();

        // Helper to extract animation keys from actions
        let extract_keys = |action: &CompiledAction| -> Option<String> {
            match action {
                CompiledAction::PlayAnim(key) => Some(key.to_string()),
                CompiledAction::Delayed { inner, .. } | CompiledAction::StateDelayed { inner, .. } => {
                    match inner.as_ref() {
                        CompiledAction::PlayAnim(key) => Some(key.to_string()),
                        _ => None,
                    }
                }
                _ => None,
            }
        };

        // Collect from state actions
        for state_actions in self.inner.logic_state_actions.iter().chain(&self.inner.movement_state_actions) {
            animation_keys.extend(state_actions.on_enter.iter().filter_map(extract_keys));
            animation_keys.extend(state_actions.tick.iter().filter_map(extract_keys));
        }

        // Collect from rule actions
        for rule in self.inner.logic_rules.iter().chain(&self.inner.movement_rules) {
            animation_keys.extend(rule.actions.iter().filter_map(extract_keys));
        }

        // Remove duplicates
        animation_keys.sort();
        animation_keys.dedup();
        animation_keys
    }
}



/// Compile an FSM asset into runtime form
pub fn compile_fsm(
    fsm: &EnemyFsm,
    archetype: Option<&crate::ai::EnemyArchetype>,
) -> Result<CompiledFsm, String> {
    // Phase 0: Extract archetype stats for $CONSTANT expansion
    let archetype_stats = archetype.and_then(|a| a.stats.as_ref());

    // Global lookup tables (filled on demand)
    let mut slot_names: Vec<String> = Vec::new();
    let mut cooldown_names: Vec<String> = Vec::new();

    // Phase 1: Build state name→ID mappings and extract actions
    let mut logic_states: HashMap<String, StateId>    = HashMap::new();
    let mut movement_states: HashMap<String, StateId> = HashMap::new();

    let mut logic_state_names    = Vec::new();
    let mut movement_state_names = Vec::new();

    let mut logic_actions_temp    = HashMap::new();
    let mut movement_actions_temp = HashMap::new();

    let (mut logic_id, mut movement_id) = (0u16, 0u16);

    for state in &fsm.state {
        match state.track.as_str() {
            "logic" => {
                logic_states.insert(state.name.clone(), logic_id);
                logic_state_names.push(state.name.clone());
                let acts = compile_state_actions(
                    state,
                    archetype,
                    archetype_stats,
                    &mut slot_names,
                    &mut cooldown_names,
                )?;
                logic_actions_temp.insert(state.name.clone(), acts);
                logic_id += 1;
            }
            "movement" => {
                movement_states.insert(state.name.clone(), movement_id);
                movement_state_names.push(state.name.clone());
                let acts = compile_state_actions(
                    state,
                    archetype,
                    archetype_stats,
                    &mut slot_names,
                    &mut cooldown_names,
                )?;
                movement_actions_temp.insert(state.name.clone(), acts);
                movement_id += 1;
            }
            other => {
                return Err(format!("Unknown track '{}' in state definition '{}'", other, state.name));
            }
        }
    }

    // Build dense action vectors (index = StateId)
    let mut logic_state_actions = vec![StateActions::empty(); logic_state_names.len()];
    for (name, actions) in logic_actions_temp {
        let idx = logic_states[&name] as usize;
        logic_state_actions[idx] = actions;
    }

    let mut movement_state_actions = vec![StateActions::empty(); movement_state_names.len()];
    for (name, actions) in movement_actions_temp {
        let idx = movement_states[&name] as usize;
        movement_state_actions[idx] = actions;
    }

    // Phase 1.5: start state resolution
    let start_logic = *logic_states
        .get(&fsm.start.logic)
        .ok_or_else(|| format!("Start logic state '{}' not found", fsm.start.logic))?;
    let start_movement = *movement_states
        .get(&fsm.start.movement)
        .ok_or_else(|| format!("Start movement state '{}' not found", fsm.start.movement))?;

    // Phase 2: transition rules
    let mut logic_rules: Vec<CompiledRule>    = Vec::new();
    let mut movement_rules: Vec<CompiledRule> = Vec::new();

    for transition in &fsm.transition {
        let target_vec = match transition.track.as_str() {
            "logic" => &mut logic_rules,
            "movement" => &mut movement_rules,
            other => return Err(format!("Unknown track '{}' in transition rule", other)),
        };

        let rule = compile_transition(
            transition,
            &logic_states,
            &movement_states,
            archetype,
            archetype_stats,
            &mut slot_names,
            &mut cooldown_names,
        )?;
        target_vec.push(rule);
    }

    logic_rules.sort_by_key(|r| r.priority);
    movement_rules.sort_by_key(|r| r.priority);

    // ---------------------------------------------------------------------
    // Spec §4 notes that the loader must add an *implicit fallback rule* so
    // that at least one rule fires every tick, preventing any possibility
    // of a state vacuum.  We inject a no-op rule that matches `Any`, has
    // `Always` as its sole condition, performs no actions, and does **not**
    // trigger a state transition.  Its priority is set to the maximum
    // `u16` value so that it is evaluated **after** all designer-authored
    // rules.
    // ---------------------------------------------------------------------

    let fallback_rule = || CompiledRule {
        from_pattern: FromPattern::Any,
        conditions: vec![CompiledCondition::Always],
        actions: Vec::new(),
        target_state: None, // Stay in current state
        priority: u16::MAX,
        terminal: false,
    };

    logic_rules.push(fallback_rule());
    movement_rules.push(fallback_rule());

    // Final assembly
    Ok(CompiledFsm {
        inner: Arc::new(CompiledFsmInner {
            start_logic,
            start_movement,
            logic_states,
            movement_states,
            logic_state_names,
            movement_state_names,
            logic_rules,
            movement_rules,
            logic_state_actions,
            movement_state_actions,
            slot_names,
            cooldown_names,
        }),
    })
}

/// Compile a single transition rule
fn compile_transition(
    transition: &FsmTransition,
    logic_states: &HashMap<String, StateId>,
    movement_states: &HashMap<String, StateId>,
    archetype: Option<&crate::ai::EnemyArchetype>,
    archetype_stats: Option<&ArchetypeStats>,
    slot_names: &mut Vec<String>,
    cooldown_names: &mut Vec<String>,
) -> Result<CompiledRule, String> {
    // Parse from pattern
    let from_pattern = parse_from_pattern(&transition.from, logic_states, movement_states)?;
    
    // Get target state
    let state_map = match transition.track.as_str() {
        "logic" => logic_states,
        "movement" => movement_states,
        _ => return Err(format!("Unknown track: {}", transition.track)),
    };
    
    let target_state = state_map.get(&transition.to)
        .copied()
        .ok_or_else(|| format!("Target state '{}' not found", transition.to))?;
    
    // Compile conditions
    let conditions = compile_conditions(&transition.conditions, archetype_stats, slot_names, cooldown_names)?;
    
    // Compile actions
    let actions = transition.actions.iter()
        .map(|a| compile_action(a, archetype, archetype_stats, cooldown_names))
        .collect::<Result<Vec<_>, _>>()?;
    
    Ok(CompiledRule {
        from_pattern,
        conditions,
        actions,
        target_state: Some(target_state),
        priority: transition.priority,
        terminal: transition.terminal,
    })
}

/// Parse from pattern string
fn parse_from_pattern(
    from: &str,
    logic_states: &HashMap<String, StateId>,
    movement_states: &HashMap<String, StateId>,
) -> Result<FromPattern, String> {
    if from == "Any" {
        return Ok(FromPattern::Any);
    }
    
    // Check if it contains pipe separator
    if from.contains('|') {
        let mut state_ids = Vec::new();
        for state_name in from.split('|') {
            let name = state_name.trim();
            // Skip empty strings
            if name.is_empty() {
                continue;
            }
            if let Some(&id) = logic_states.get(name).or_else(|| movement_states.get(name)) {
                state_ids.push(id);
            } else {
                return Err(format!("State '{}' not found in from pattern", name));
            }
        }
        if state_ids.is_empty() {
            return Err("From pattern contains no valid states".to_string());
        }
        Ok(FromPattern::Multiple(state_ids))
    } else {
        // Single state
        if let Some(&id) = logic_states.get(from).or_else(|| movement_states.get(from)) {
            Ok(FromPattern::Specific(id))
        } else {
            Err(format!("State '{}' not found", from))
        }
    }
}

/// Generic value parser that handles constant references
fn parse_value<F>(
    value: &serde_json::Value,
    archetype_stats: Option<&ArchetypeStats>,
    const_resolver: F,
) -> Result<f32, String> 
where
    F: Fn(&ArchetypeStats, &str) -> Option<f32>,
{
    if let Some(n) = value.as_f64() {
        Ok(n as f32)
    } else if let Some(s) = value.as_str() {
        if s.starts_with('$') {
            let const_name = &s[1..];
            if let Some(stats) = archetype_stats {
                const_resolver(stats, const_name)
                    .ok_or_else(|| format!("Unknown constant: {}", const_name))
            } else {
                Err(format!("No archetype stats available for constant: {}", const_name))
            }
        } else {
            s.parse::<f32>()
                .map_err(|_| format!("Invalid numeric value: {}", s))
        }
    } else {
        Err(format!("Invalid value: {:?}", value))
    }
}

/// Compile conditions into a list of condition data
fn compile_conditions(
    conditions: &[FsmCondition],
    archetype_stats: Option<&ArchetypeStats>,
    slot_names: &mut Vec<String>,
    cooldown_names: &mut Vec<String>,
) -> Result<Vec<CompiledCondition>, String> {
    if conditions.is_empty() {
        // Empty conditions means always true
        return Ok(vec![CompiledCondition::Always]);
    }
    
    let mut compiled_conditions: Vec<CompiledCondition> = Vec::new();
    
    for condition in conditions {
        let compiled = compile_single_condition(condition, archetype_stats, slot_names, cooldown_names)?;
        compiled_conditions.push(compiled);
    }
    
    Ok(compiled_conditions)
}

/// Compile a single condition
fn compile_single_condition(
    condition: &FsmCondition,
    archetype_stats: Option<&ArchetypeStats>,
    slot_names: &mut Vec<String>,
    cooldown_names: &mut Vec<String>,
) -> Result<CompiledCondition, String> {
    match condition {
        FsmCondition::Simple(name) => {
            match name.as_str() {
                "HealthZero" => Ok(CompiledCondition::HealthZero),
                _ => Err(format!("Unknown condition: {}", name)),
            }
        },
        FsmCondition::WithParam { params } => {
            // Distance conditions
            if let Some(distance_lt) = params.get("distance_lt") {
                let distance = parse_distance_value(distance_lt, archetype_stats)?;
                return Ok(CompiledCondition::DistanceLt(distance));
            }
            
            if let Some(distance_gt) = params.get("distance_gt") {
                let distance = parse_distance_value(distance_gt, archetype_stats)?;
                return Ok(CompiledCondition::DistanceGt(distance));
            }
            
            // Timer condition
            if let Some(timer_gt) = params.get("TimerGt").and_then(|v| v.as_u64()) {
                return Ok(CompiledCondition::TimerGt(timer_gt as u16));
            }
            
            // Grounded condition
            if let Some(expected) = params.get("IsGrounded").and_then(|v| v.as_bool()) {
                return Ok(CompiledCondition::IsGrounded(expected));
            }
            
            // RngChance
            if let Some(rng_val) = params.get("RngChance") {
                let p = parse_u8(rng_val, "RngChance")?;
                return Ok(CompiledCondition::RngChance(p));
            }

            // Slot condition
            if let Some(slot_val) = params.get("Slot") {
                let (name, expected) = if let Some(obj) = slot_val.as_object() {
                    let name = obj.get("name")
                        .and_then(|v| v.as_str())
                        .ok_or("Slot condition missing name")?;
                    let expected = obj.get("bool")
                        .or_else(|| obj.get("value"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    (name, expected)
                } else if let Some(name_str) = slot_val.as_str() {
                    (name_str, true)
                } else {
                    return Err("Invalid Slot condition format".to_string());
                };
                
                let id = register_or_get_id(name, slot_names, 32)?;
                return Ok(CompiledCondition::Slot { id, expected });
            }

            // CooldownReady
            if let Some(name) = params.get("CooldownReady").and_then(|v| v.as_str()) {
                let id = register_or_get_id(name, cooldown_names, 255)?;
                return Ok(CompiledCondition::CooldownReady(id));
            }

            Err(format!("Unknown parameterized condition: {:?}", params))
        }
    }
}

/// Helper to parse distance values with constant support
fn parse_distance_value(
    value: &serde_json::Value,
    archetype_stats: Option<&ArchetypeStats>,
) -> Result<f32, String> {
    parse_value(value, archetype_stats, |stats, name| {
        match name {
            "VISION_RANGE" => Some(stats.vision_range),
            "MELEE_RANGE" => Some(stats.melee_range),
            _ => None,
        }
    })
}

/// Helper to parse u8 values
fn parse_u8(val: &serde_json::Value, field_name: &str) -> Result<u8, String> {
    if let Some(n) = val.as_u64() {
        Ok(n as u8)
    } else if let Some(s) = val.as_str() {
        s.parse::<u8>().map_err(|_| format!("Invalid {} value: {}", field_name, s))
    } else {
        Err(format!("Invalid {} value: {:?}", field_name, val))
    }
}

/// Helper to register a name in a vector and get its ID
fn register_or_get_id(name: &str, names: &mut Vec<String>, max: usize) -> Result<u8, String> {
    if let Some(idx) = names.iter().position(|s| s == name) {
        Ok(idx as u8)
    } else {
        if names.len() >= max {
            return Err(format!("Exceeded maximum of {} names", max));
        }
        names.push(name.to_string());
        Ok((names.len() - 1) as u8)
    }
}

/// Compile a single action
fn compile_action(
    action: &FsmAction,
    archetype: Option<&crate::ai::EnemyArchetype>,
    archetype_stats: Option<&ArchetypeStats>,
    cooldown_names: &mut Vec<String>,
) -> Result<CompiledAction, String> {
    match action {
        FsmAction::Simple(name) => {
            match name.as_str() {
                "face_player" => Ok(CompiledAction::FacePlayer),
                _ => Err(format!("Unknown simple action: {}", name)),
            }
        }
        FsmAction::WithParam { params } => {
            // Handle delayed actions first
            if let Some(tick_val) = params.get("at_tick").or_else(|| params.get("at_anim_tick")) {
                let tick = parse_u16(tick_val, "at_anim_tick")?;
                let mut inner_params = params.clone();
                inner_params.remove("at_tick");
                inner_params.remove("at_anim_tick");
                
                if inner_params.is_empty() {
                    return Err("at_anim_tick provided but no inner action specified".to_string());
                }
                
                let inner = compile_action(&FsmAction::WithParam { params: inner_params }, archetype, archetype_stats, cooldown_names)?;
                return Ok(CompiledAction::Delayed { tick, inner: Box::new(inner) });
            }
            
            if let Some(tick_val) = params.get("at_state_tick") {
                let tick = parse_u16(tick_val, "at_state_tick")?;
                let mut inner_params = params.clone();
                inner_params.remove("at_state_tick");
                
                if inner_params.is_empty() {
                    return Err("at_state_tick provided but no inner action specified".to_string());
                }
                
                let inner = compile_action(&FsmAction::WithParam { params: inner_params }, archetype, archetype_stats, cooldown_names)?;
                return Ok(CompiledAction::StateDelayed { tick, inner: Box::new(inner) });
            }

            // Parse other actions
            if let Some(anim) = params.get("play_anim").and_then(|v| v.as_str()) {
                let full_key = if anim.starts_with("anim.") {
                    anim.to_string()
                } else {
                    let arch = archetype.ok_or_else(|| format!("Cannot resolve play_anim '{}' – no archetype context provided", anim))?;
                    arch.anim.get(anim)
                        .cloned()
                        .ok_or_else(|| format!("Animation key '{}' missing in archetype {}", anim, arch.id))?
                };
                return Ok(CompiledAction::PlayAnim(Cow::Owned(full_key)));
            }
            
            if let Some(vel) = params.get("set_vel").and_then(|v| v.as_str()) {
                let parts: Vec<&str> = vel.split(',').collect();
                if parts.len() == 2 {
                    let x = parts[0].trim().parse::<f32>()
                        .map_err(|_| format!("Invalid velocity X: {}", parts[0]))?;
                    let y = parts[1].trim().parse::<f32>()
                        .map_err(|_| format!("Invalid velocity Y: {}", parts[1]))?;
                    return Ok(CompiledAction::SetVel(Vec2::new(x, y)));
                }
                return Err("set_vel requires X,Y format".to_string());
            }
            
            if let Some(speed_val) = params.get("set_vel_towards_player") {
                let speed = parse_value(speed_val, archetype_stats, |stats, name| {
                    match name {
                        "CHASE_SPEED" => Some(stats.chase_speed),
                        "WALK_SPEED" => Some(stats.walk_speed),
                        _ => None,
                    }
                })?;
                return Ok(CompiledAction::SetVelTowardsPlayer(speed));
            }
            
            if let Some(speed_val) = params.get("set_vel_from_facing") {
                let speed = parse_value(speed_val, archetype_stats, |stats, name| {
                    match name {
                        "CHASE_SPEED" => Some(stats.chase_speed),
                        "WALK_SPEED" => Some(stats.walk_speed),
                        _ => None,
                    }
                })?;
                return Ok(CompiledAction::SetVelFromFacing(speed));
            }
            
            if let Some(attack_params) = params.get("spawn_attack").and_then(|v| v.as_object()) {
                let key = attack_params.get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("spawn_attack missing key")?
                    .to_string();
                
                let dmg_val = attack_params.get("dmg")
                    .ok_or("spawn_attack missing dmg")?;
                let dmg = parse_value(dmg_val, archetype_stats, |stats, name| {
                    match name {
                        "DMG_MELEE" => Some(stats.dmg_melee as f32),
                        "DMG_RANGED" => Some(stats.dmg_ranged as f32),
                        _ => None,
                    }
                })?;
                
                return Ok(CompiledAction::SpawnAttack { key, dmg });
            }
            
            if let Some(cd_params) = params.get("Cooldown").and_then(|v| v.as_object()) {
                let name = cd_params.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Cooldown action missing name")?;
                let ticks_val = cd_params.get("ticks")
                    .ok_or("Cooldown action missing ticks")?;
                let ticks = parse_u16(ticks_val, "ticks")?;

                if !cooldown_names.contains(&name.to_string()) {
                    cooldown_names.push(name.to_string());
                }
                return Ok(CompiledAction::Cooldown { name: name.to_string(), ticks });
            }
            
            if params.get("face_player").and_then(|v| v.as_bool()) == Some(true) {
                return Ok(CompiledAction::FacePlayer);
            }
            
            Err(format!("Unknown parameterized action: {:?}", params))
        }
    }
}

/// Helper to parse u16 values
fn parse_u16(val: &serde_json::Value, field_name: &str) -> Result<u16, String> {
    if let Some(n) = val.as_u64() {
        Ok(n as u16)
    } else if let Some(s) = val.as_str() {
        s.parse::<u16>().map_err(|_| format!("Invalid {} value: {}", field_name, s))
    } else {
        Err(format!("Invalid {} value: {:?}", field_name, val))
    }
}

/// Compile on_enter and tick actions for a state definition
fn compile_state_actions(
    state: &FsmState,
    archetype: Option<&crate::ai::EnemyArchetype>,
    archetype_stats: Option<&ArchetypeStats>,
    slot_names: &mut Vec<String>,
    cooldown_names: &mut Vec<String>,
) -> Result<StateActions, String> {
    let mut on_enter_compiled = Vec::new();
    let mut tick_compiled = Vec::new();

    // Process on_enter actions. Any Delayed/StateDelayed wrapper – including tick == 0 – is
    // routed to the per-frame tick array so that it can re-fire on every animation loop as
    // required by spec §7.2.1.  All other actions execute immediately on state entry.
    for a in &state.on_enter {
        let compiled = compile_action(a, archetype, archetype_stats, cooldown_names)?;
        match compiled {
            CompiledAction::Delayed { .. } | CompiledAction::StateDelayed { .. } => {
                tick_compiled.push(compiled);
            },
            _ => on_enter_compiled.push(compiled),
        }
    }

    // Process tick actions (including delayed)
    for a in &state.tick {
        tick_compiled.push(compile_action(a, archetype, archetype_stats, cooldown_names)?);
    }

    Ok(StateActions {
        on_enter: on_enter_compiled,
        tick: tick_compiled,
    })
} 