// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use clipper2_rust::{
    EndType, FillRule, JoinType, PathD, PathsD, PointD, inflate_paths_d, intersect_d, union_d,
};

use crate::geometry::Contour;

const PRECISION: i32 = 3;

#[derive(Clone, Copy)]
pub enum Fill {
    EvenOdd,
    NonZero,
}

impl From<Fill> for FillRule {
    fn from(f: Fill) -> Self {
        match f {
            Fill::EvenOdd => FillRule::EvenOdd,
            Fill::NonZero => FillRule::NonZero,
        }
    }
}

fn to_paths(contours: &[Contour]) -> PathsD {
    contours
        .iter()
        .map(|poly| poly.iter().map(|&(x, y)| PointD::new(x, y)).collect::<PathD>())
        .collect()
}

fn from_paths(paths: &PathsD) -> Vec<Contour> {
    paths
        .iter()
        .filter(|p| p.len() >= 3)
        .map(|p| p.iter().map(|pt| (pt.x, pt.y)).collect())
        .collect()
}

pub fn intersect(subject: &[Contour], clip: &[Contour], fill: Fill) -> Vec<Contour> {
    from_paths(&intersect_d(&to_paths(subject), &to_paths(clip), fill.into(), PRECISION))
}

pub fn union(contours: &[Contour]) -> Vec<Contour> {
    from_paths(&union_d(&to_paths(contours), &PathsD::new(), FillRule::NonZero, PRECISION))
}

pub fn inflate(contours: &[Contour], delta: f64) -> Vec<Contour> {
    from_paths(&inflate_paths_d(
        &to_paths(contours),
        delta,
        JoinType::Round,
        EndType::Polygon,
        2.0,
        PRECISION,
        0.0,
    ))
}
