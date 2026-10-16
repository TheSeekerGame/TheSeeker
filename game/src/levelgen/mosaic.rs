//! Stable terrain mosaics and ordered generation-rule evaluation.
use super::*;

const FLIPS: [BVec2; 4] = [BVec2::FALSE, BVec2::new(true, false), BVec2::new(false, true), BVec2::TRUE];

pub(super) fn run_mosaic_generative_layers(
    compiled: &CompiledLevel, mosaics: &[MosaicRules; 3], images: &Assets<Image>,
) -> [Vec<bool>; 3] {
    let _timer = diagnostics::StageTimer::new("run_mosaic_generative_layers");
    let mut outputs: [Vec<bool>; 3] = std::array::from_fn(|_| Vec::new());
    for index in 0..3 {
        let source = if index == 0 { &compiled.terrain } else { &outputs[index - 1] };
        let layer = apply_mosaic_layer_rules(source, &compiled.terrain, compiled, &mosaics[index], images);
        // The highest matching layer owns each cell.
        for lower in &mut outputs[..index] {
            lower.iter_mut().zip(&layer).filter(|(_, claimed)| **claimed).for_each(|(cell, _)| *cell = false);
        }
        outputs[index] = layer;
    }
    outputs
}

/// Runs a layer's rules in order over `source`. Each rule claims matching source cells into
/// the layer, so later rules see only what earlier ones left.
pub(super) fn apply_mosaic_layer_rules(
    source: &[bool],
    primary: &[bool],
    compiled: &CompiledLevel,
    rules: &MosaicRules,
    images: &Assets<Image>,
) -> Vec<bool> {
    let decoded = rules.decoded.get_or_init(|| {
        let mut decoded: Vec<_> = rules.rules.iter().filter_map(|entry| decode_rule(entry, images)).collect();
        decoded.sort_by_key(|rule| rule.order);
        decoded
    });
    let mut layer = vec![false; source.len()];
    let mut unclaimed = source.to_vec();
    for rule in decoded {
        apply_rule(rule, primary, compiled, &mut layer, &mut unclaimed);
    }
    layer
}

fn apply_rule(rule: &MosaicDecodedRule, primary: &[bool], compiled: &CompiledLevel, layer: &mut [bool], unclaimed: &mut [bool]) {
    let (source, layer_before) = (unclaimed.to_vec(), layer.to_vec());
    let view = RuleView { width: compiled.width, height: compiled.height, source: &source, primary, layer: &layer_before };
    // A center requirement holds under any flip, so it rejects cells up front.
    let needs_center = rule.source.require.first() == Some(&(0, 0));
    let cells = (0..compiled.height as i32).flat_map(|y| (0..compiled.width as i32).map(move |x| IVec2::new(x, y)));
    for cell in cells {
        if needs_center && !source[view.index(cell).expect("cells lie in the grid")] {
            continue;
        }
        let Some(claims) = view.claims(rule, cell, unclaimed) else { continue; };
        for index in claims {
            layer[index] = true;
            unclaimed[index] = false;
        }
    }
}

/// The grids a rule reads, as they were when the rule started.
struct RuleView<'a> {
    width: u32,
    height: u32,
    source: &'a [bool],
    primary: &'a [bool],
    layer: &'a [bool],
}

impl RuleView<'_> {
    fn index(&self, cell: IVec2) -> Option<usize> {
        let inside = cell.x >= 0 && cell.y >= 0 && cell.x < self.width as i32 && cell.y < self.height as i32;
        inside.then(|| (cell.y as u32 * self.width + cell.x as u32) as usize)
    }

    /// The grid index of `offset` from `cell`, mirrored per `flip`.
    fn at(&self, cell: IVec2, (dx, dy): (i32, i32), flip: BVec2) -> Option<usize> {
        self.index(cell + IVec2::new(if flip.x { -dx } else { dx }, if flip.y { -dy } else { dy }))
    }

    fn set(&self, grid: &[bool], cell: IVec2, offset: (i32, i32), flip: BVec2) -> bool {
        self.at(cell, offset, flip).is_some_and(|index| grid.get(index) == Some(&true))
    }

    fn matches(&self, rule: &MosaicDecodedRule, cell: IVec2, flip: BVec2) -> bool {
        [(self.source, &rule.source), (self.primary, &rule.primary), (self.layer, &rule.layer)]
            .into_iter()
            .all(|(grid, conditions)| {
                conditions.require.iter().all(|&offset| self.set(grid, cell, offset, flip))
                    && !conditions.forbid.iter().any(|&offset| self.set(grid, cell, offset, flip))
            })
    }

    /// The cells `rule` claims when centered on `cell`, if it fires there: the first allowed
    /// orientation that matches, its required source still unclaimed, then a probability roll
    /// that may also mirror the claimed cells. Every claimed cell must still be unclaimed.
    fn claims(&self, rule: &MosaicDecodedRule, cell: IVec2, unclaimed: &[bool]) -> Option<Vec<usize>> {
        let flip = FLIPS
            .into_iter()
            .filter(|flip| (!flip.x || rule.ruleflip_x) && (!flip.y || rule.ruleflip_y))
            .find(|&flip| self.matches(rule, cell, flip))?;
        if !rule.source.require.iter().all(|&offset| self.set(unclaimed, cell, offset, flip)) {
            return None;
        }
        let hash = tile_hash(cell.x, cell.y, rule.order, 0);
        if !within_percent(hash, rule.prob_percent) {
            return None;
        }
        let claim_flip = BVec2::new(
            flip.x || rule.randflip_x && (hash >> 1) & 1 == 1,
            flip.y || rule.randflip_y && (hash >> 2) & 1 == 1,
        );
        rule.claims
            .iter()
            .map(|&offset| self.at(cell, offset, claim_flip).filter(|&index| unclaimed[index]))
            .collect()
    }
}

/// Decodes a rule mask: each opaque pixel's color adds its offset from the center to one
/// of the rule's conditions. Rules that claim nothing are dropped.
fn decode_rule(entry: &RuleImage, images: &Assets<Image>) -> Option<MosaicDecodedRule> {
    let image = images.get(&entry.handle)?;
    let mut rule = MosaicDecodedRule {
        order: entry.order,
        source: MaskConditions::default(),
        primary: MaskConditions::default(),
        layer: MaskConditions::default(),
        claims: Vec::new(),
        ruleflip_x: entry.ruleflip_x,
        ruleflip_y: entry.ruleflip_y,
        randflip_x: entry.randflip_x,
        randflip_y: entry.randflip_y,
        prob_percent: entry.prob_percent,
    };
    let width = image.width() as i32;
    let center = ((image.width() / 2) as i32, (image.height() / 2) as i32);
    let pixels = image.data.as_deref().unwrap_or_default().as_chunks::<4>().0;
    let opaque = pixels.iter().take((image.width() * image.height()) as usize).enumerate()
        .filter(|(_, pixel)| pixel[3] >= 128);
    for (index, pixel) in opaque {
        let offset = (index as i32 % width - center.0, index as i32 / width - center.1);
        rule.add_mask_pixel([pixel[0], pixel[1], pixel[2]], offset);
    }
    // Nearest requirements first, so most rejections are cheap.
    rule.source.require.sort_by_key(|&(x, y)| x.abs() + y.abs());
    (!rule.claims.is_empty()).then_some(rule)
}

impl MosaicDecodedRule {
    fn add_mask_pixel(&mut self, rgb: [u8; 3], offset: (i32, i32)) {
        match rgb {
            // Black cells must be source, and are what the rule claims.
            [0, 0, 0] => {
                self.source.require.push(offset);
                self.claims.push(offset);
            },
            compiler::COLOR_RULE_REQUIRE => self.source.require.push(offset),
            compiler::COLOR_RULE_FORBID => self.source.forbid.push(offset),
            compiler::MOSAIC_RULE_REQUIRE_PRIMARY_COLOR => self.primary.require.push(offset),
            compiler::MOSAIC_RULE_FORBID_PRIMARY_COLOR => self.primary.forbid.push(offset),
            compiler::MOSAIC_RULE_REQUIRE_SELF_COLOR => self.layer.require.push(offset),
            compiler::MOSAIC_RULE_FORBID_SELF_COLOR => self.layer.forbid.push(offset),
            _ => {},
        }
    }
}
