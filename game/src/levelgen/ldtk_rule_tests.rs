use super::*;

#[test]
fn prepared_priority_program_preserves_authored_rules_flips_probability_and_breaks() {
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read("assets/levels/tiles/ldtk_rules/level.01_test.ldtk").unwrap()).unwrap();
    let layers = json["defs"]["layers"].as_array().unwrap();
    for layer in layers.iter().filter_map(parse_layer) { compare_program(&layer); }
}

fn compare_program(layer: &LdtkAutoLayer) {
    let grid: Vec<_> = (0..899).map(|i| ((i * 73 + i / 31) % 13) - 2).collect();
    let allowed: [&str; 2] = ["wall", "fog"];
    for filter in [None, Some(allowed.as_slice())] {
        let actual = RuleProgram::new(&grid, UVec2::new(31, 29), layer, 731, &[filter], true).cells();
        let ctx = ApplyRulesContext { int_grid: &grid, width: 31, height: 29, det_seed: 731 };
        let expected: Vec<_> = (0..899).map(|i| reference_cell(&ctx, layer, filter, i % 31, i / 31)).collect();
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "layer {}", layer.identifier);
    }
    let ctx = ApplyRulesContext { int_grid: &grid, width: 31, height: 29, det_seed: 731 };
    compare_priorities(&ctx, layer, &[
        Some(&["spikes"]), Some(&["wall0", "wall1", "wall2", "wall3"]),
        Some(&["fog1", "fog2", "fog3"]), Some(&["fog_add"]),
    ], RuleProgram::terrain(&grid, UVec2::new(31, 29), layer, 731));
    compare_priorities(&ctx, layer, &[Some(&["fog"]), Some(&["wall"])],
        RuleProgram::background(&grid, UVec2::new(31, 29), layer, 731));
}

fn compare_priorities(ctx: &ApplyRulesContext<'_>, layer: &LdtkAutoLayer,
    stages: &[Option<&[&str]>], program: RuleProgram<'_>,
) {
    for (index, &value) in ctx.int_grid.iter().enumerate() {
        let expected = (value != 0).then(|| stages.iter().find_map(|&stage| {
            reference_cell(ctx, layer, stage, (index % ctx.width as usize) as i32, (index / ctx.width as usize) as i32)
        })).flatten();
        assert_eq!(program.at(index), expected, "layer {}, index {index}", layer.identifier);
    }
}

fn reference_cell(ctx: &ApplyRulesContext<'_>, layer: &LdtkAutoLayer, allowed: Option<&[&str]>, x: i32, y: i32) -> Option<TilePlacement> {
    let mut output = None;
    let rules = layer.rule_groups.iter().filter(|g| rule_group_is_allowed(g, allowed)).flat_map(|g| &g.rules);
    for rule in rules.filter(|r| r.active && !r.tile_ids.is_empty()) {
        if reference_rule(ctx, rule, x, y, &mut output) && rule.break_on_match { break; }
    }
    output
}

fn reference_rule(ctx: &ApplyRulesContext<'_>, rule: &LdtkRule, x: i32, y: i32, output: &mut Option<TilePlacement>) -> bool {
    for &(flip_x, flip_y) in flip_combinations(rule) {
        if !reference_matches(ctx, rule, IVec2::new(x, y), flip_x, flip_y) { continue; }
        if rule_chance_passes(ctx, rule, x, y) {
            *output = Some(TilePlacement { tile_id: choose_rule_tile_id(ctx, rule, x, y), flip_x, flip_y });
            return true;
        }
    }
    false
}

fn reference_matches(ctx: &ApplyRulesContext<'_>, rule: &LdtkRule, center: IVec2, flip_x: bool, flip_y: bool) -> bool {
    let size = i32::from(rule.size);
    rule.pattern.iter().enumerate().all(|(i, &expected)| {
        let delta = IVec2::new(i as i32 % size - size / 2, i as i32 / size - size / 2);
        let position = center + delta * IVec2::new([1, -1][usize::from(flip_x)], [1, -1][usize::from(flip_y)]);
        let actual = reference_grid_value(ctx, position);
        expected_matches_actual(expected, actual)
    })
}

fn reference_grid_value(ctx: &ApplyRulesContext<'_>, position: IVec2) -> i32 {
    if position.x < 0 || position.y < 0 || position.x >= ctx.width as i32 || position.y >= ctx.height as i32 { return 0; }
    ctx.int_grid[(position.y as u32 * ctx.width + position.x as u32) as usize]
}
