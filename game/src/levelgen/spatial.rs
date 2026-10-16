//! Sparse placement ownership and bounded spacing queries.
use super::*;

#[derive(Default)]
pub(super) struct PlacementIndex {
    cells: HashMap<IVec2, Vec<IVec2>>,
}

const CELL_SIZE: i32 = 32;

impl PlacementIndex {
    pub fn insert(&mut self, position: IVec2) {
        self.cells.entry(cell(position)).or_default().push(position);
    }

    pub fn clear_square(&self, position: IVec2, distance: i32) -> bool {
        let min = cell(position - IVec2::splat(distance));
        let max = cell(position + IVec2::splat(distance));
        (min.y..=max.y).all(|y| clear_row(&self.cells, position, distance, min.x..=max.x, y))
    }
}

fn cell(position: IVec2) -> IVec2 {
    IVec2::new(position.x.div_euclid(CELL_SIZE), position.y.div_euclid(CELL_SIZE))
}

fn clear_row(cells: &HashMap<IVec2, Vec<IVec2>>, position: IVec2, distance: i32, columns: std::ops::RangeInclusive<i32>, y: i32) -> bool {
    columns.filter_map(|x| cells.get(&IVec2::new(x, y))).flatten()
        .all(|&other| (position - other).abs().max_element() > distance)
}
