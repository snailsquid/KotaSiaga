//! SelWilayah: fixed 500 m grid over the service area. `cell_id` is `c_<row>_<col>`.

use std::fmt;

use crate::rules::{CELL_SIZE_M, GRID_REF_LAT, SERVICE_AREA as A};

const M_PER_DEG_LAT: f64 = 111_320.0;

fn m_per_deg_lon() -> f64 {
    M_PER_DEG_LAT * GRID_REF_LAT.to_radians().cos()
}

pub fn rows() -> u32 {
    ((A.max_lat - A.min_lat) * M_PER_DEG_LAT / CELL_SIZE_M).ceil() as u32
}

pub fn cols() -> u32 {
    ((A.max_lon - A.min_lon) * m_per_deg_lon() / CELL_SIZE_M).ceil() as u32
}

pub fn cell_count() -> usize {
    rows() as usize * cols() as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellId {
    pub row: u32,
    pub col: u32,
}

impl CellId {
    pub fn of(lat: f64, lon: f64) -> Option<Self> {
        if !A.contains(lat, lon) {
            return None;
        }
        let row =
            (((lat - A.min_lat) * M_PER_DEG_LAT / CELL_SIZE_M).floor() as u32).min(rows() - 1);
        let col =
            (((lon - A.min_lon) * m_per_deg_lon() / CELL_SIZE_M).floor() as u32).min(cols() - 1);
        Some(Self { row, col })
    }

    pub fn index(self) -> usize {
        self.row as usize * cols() as usize + self.col as usize
    }

    pub fn from_index(i: usize) -> Self {
        let c = cols() as usize;
        Self {
            row: (i / c) as u32,
            col: (i % c) as u32,
        }
    }

    pub fn center(self) -> (f64, f64) {
        let lat = A.min_lat + (self.row as f64 + 0.5) * CELL_SIZE_M / M_PER_DEG_LAT;
        let lon = A.min_lon + (self.col as f64 + 0.5) * CELL_SIZE_M / m_per_deg_lon();
        (lat, lon)
    }

    /// This cell and its up to 8 neighbors.
    pub fn with_neighbors(self) -> impl Iterator<Item = CellId> {
        let (r, c, rs, cs) = (
            self.row as i64,
            self.col as i64,
            rows() as i64,
            cols() as i64,
        );
        (-1..=1)
            .flat_map(move |dr| (-1..=1).map(move |dc| (r + dr, c + dc)))
            .filter_map(move |(rr, cc)| {
                (rr >= 0 && cc >= 0 && rr < rs && cc < cs).then_some(CellId {
                    row: rr as u32,
                    col: cc as u32,
                })
            })
    }
}

impl fmt::Display for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "c_{}_{}", self.row, self.col)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bbox {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

impl Bbox {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        (self.min_lat..=self.max_lat).contains(&lat) && (self.min_lon..=self.max_lon).contains(&lon)
    }

    /// Cells overlapping the box, clipped to the service area. `None` if more than `max` cells.
    pub fn cells(&self, max: usize) -> Option<Vec<CellId>> {
        let clamp_lat = |v: f64| v.clamp(A.min_lat, A.max_lat);
        let clamp_lon = |v: f64| v.clamp(A.min_lon, A.max_lon);
        if self.max_lat < A.min_lat
            || self.min_lat > A.max_lat
            || self.max_lon < A.min_lon
            || self.min_lon > A.max_lon
        {
            return Some(Vec::new());
        }
        let lo = CellId::of(clamp_lat(self.min_lat), clamp_lon(self.min_lon))?;
        let hi = CellId::of(clamp_lat(self.max_lat), clamp_lon(self.max_lon))?;
        let n = (hi.row - lo.row + 1) as usize * (hi.col - lo.col + 1) as usize;
        if n > max {
            return None;
        }
        Some(
            (lo.row..=hi.row)
                .flat_map(|row| (lo.col..=hi.col).map(move |col| CellId { row, col }))
                .collect(),
        )
    }
}

/// Equirectangular distance; accurate enough at city scale.
pub fn distance_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dy = (a.0 - b.0) * M_PER_DEG_LAT;
    let dx = (a.1 - b.1) * m_per_deg_lon();
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_covers_greater_bandung_in_500m_cells() {
        assert_eq!((rows(), cols()), (78, 78));
    }

    #[test]
    fn center_maps_back_to_same_cell() {
        let c = CellId::of(-6.89, 107.61).unwrap();
        let (lat, lon) = c.center();
        assert_eq!(CellId::of(lat, lon), Some(c));
        assert_eq!(CellId::from_index(c.index()), c);
        assert!(distance_m((lat, lon), (-6.89, 107.61)) < 360.0);
    }

    #[test]
    fn outside_area_has_no_cell() {
        assert_eq!(CellId::of(-6.2, 106.8), None);
    }

    #[test]
    fn corner_cell_has_four_with_neighbors() {
        assert_eq!(CellId { row: 0, col: 0 }.with_neighbors().count(), 4);
        assert_eq!(CellId { row: 5, col: 5 }.with_neighbors().count(), 9);
    }

    #[test]
    fn bbox_cell_limit() {
        let whole = Bbox {
            min_lon: 107.0,
            min_lat: -8.0,
            max_lon: 108.0,
            max_lat: -6.0,
        };
        assert!(whole.cells(2500).is_none());
        let small = Bbox {
            min_lon: 107.60,
            min_lat: -6.90,
            max_lon: 107.61,
            max_lat: -6.89,
        };
        assert!(small.cells(2500).unwrap().len() <= 9);
    }
}
