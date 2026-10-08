// SPDX-FileCopyrightText: 2026 Froststrap
//
// SPDX-License-Identifier: MPL-2.0

use skrifa::outline::OutlinePen;

use crate::clip::{self, Fill};

pub type Pt = (f64, f64);
pub type Contour = Vec<Pt>;
pub type IntContour = Vec<(i16, i16)>;

const BEZIER_STEPS: usize = 12;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    fn pick(self, p: Pt) -> f64 {
        if self == Axis::X { p.0 } else { p.1 }
    }
    pub fn other(self) -> Axis {
        if self == Axis::X { Axis::Y } else { Axis::X }
    }
}

#[derive(Clone, Copy)]
pub enum Slicing {
    Axis(Axis),
    Rotated(f64),
}

impl Slicing {
    pub fn from_angle(angle: i32) -> Self {
        match angle.rem_euclid(360) {
            0 | 180 => Self::Axis(Axis::Y),
            90 | 270 => Self::Axis(Axis::X),
            a => Self::Rotated(a as f64),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BBox {
    pub x_min: f64,
    pub y_max: f64,
    pub w: f64,
    pub h: f64,
}

impl BBox {
    pub fn of(contours: &[Contour]) -> Option<Self> {
        let (x0, x1) = extent(contours, Axis::X)?;
        let (y0, y1) = extent(contours, Axis::Y)?;
        Some(Self { x_min: x0, y_max: y1, w: x1 - x0, h: y1 - y0 })
    }

    pub fn em(units: u16) -> Self {
        let u = units as f64;
        Self { x_min: 0.0, y_max: u, w: u, h: u }
    }
}

#[derive(Default)]
pub struct Flattener {
    done: Vec<Contour>,
    cur: Contour,
}

impl Flattener {
    fn commit(&mut self) {
        let cur = std::mem::take(&mut self.cur);
        if cur.len() >= 3 {
            self.done.push(cur);
        }
    }
    pub fn finish(mut self) -> Vec<Contour> {
        self.commit();
        self.done
    }
    fn last(&self) -> Pt {
        self.cur.last().copied().unwrap_or((0.0, 0.0))
    }
}

impl OutlinePen for Flattener {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commit();
        self.cur.push((x.into(), y.into()));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cur.push((x.into(), y.into()));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.last(), (cx as f64, cy as f64), (x as f64, y as f64));
        for i in 1..=BEZIER_STEPS {
            let t = i as f64 / BEZIER_STEPS as f64;
            let m = 1.0 - t;
            self.cur.push((
                m * m * p0.0 + 2.0 * m * t * p1.0 + t * t * p2.0,
                m * m * p0.1 + 2.0 * m * t * p1.1 + t * t * p2.1,
            ));
        }
    }
    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let p0 = self.last();
        let (p1, p2, p3) = ((c0x as f64, c0y as f64), (c1x as f64, c1y as f64), (x as f64, y as f64));
        for i in 1..=BEZIER_STEPS {
            let t = i as f64 / BEZIER_STEPS as f64;
            let m = 1.0 - t;
            self.cur.push((
                m.powi(3) * p0.0 + 3.0 * m * m * t * p1.0 + 3.0 * m * t * t * p2.0 + t.powi(3) * p3.0,
                m.powi(3) * p0.1 + 3.0 * m * m * t * p1.1 + 3.0 * m * t * t * p2.1 + t.powi(3) * p3.1,
            ));
        }
    }
    fn close(&mut self) {
        self.commit();
    }
}

pub fn rotate(contours: &[Contour], deg: f64) -> Vec<Contour> {
    let (s, c) = deg.to_radians().sin_cos();
    contours
        .iter()
        .map(|poly| poly.iter().map(|&(x, y)| (x * c - y * s, x * s + y * c)).collect())
        .collect()
}

pub fn extent(contours: &[Contour], axis: Axis) -> Option<(f64, f64)> {
    let mut it = contours.iter().flatten().map(|&p| axis.pick(p));
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v))))
}

pub fn clip_band(
    contours: &[Contour],
    axis: Axis,
    lo: f64,
    hi: f64,
    cross: (f64, f64),
) -> Vec<Contour> {
    let subject: Vec<Contour> = contours
        .iter()
        .filter(|poly| {
            matches!(
                extent(std::slice::from_ref(*poly), axis),
                Some((mn, mx)) if mx >= lo && mn <= hi
            )
        })
        .cloned()
        .collect();
    if subject.is_empty() {
        return vec![];
    }

    let (c0, c1) = (cross.0 - 1000.0, cross.1 + 1000.0);
    let rect: Contour = match axis {
        Axis::Y => vec![(c0, lo), (c1, lo), (c1, hi), (c0, hi)],
        Axis::X => vec![(lo, c0), (hi, c0), (hi, c1), (lo, c1)],
    };
    clip::intersect(&subject, &[rect], Fill::EvenOdd)
}


pub fn to_int_contour(poly: &Contour) -> Option<IntContour> {
    let mut dedup: Vec<(i32, i32)> = Vec::new();
    for &(x, y) in poly {
        let p = (x.round() as i32, y.round() as i32);
        if dedup.last() != Some(&p) {
            dedup.push(p);
        }
    }
    if dedup.len() > 1 && dedup.first() == dedup.last() {
        dedup.pop();
    }
    if dedup.len() < 3 {
        return None;
    }

    let mut s = vec![dedup[0]];
    for i in 1..dedup.len() - 1 {
        let (p1, p2, p3) = (*s.last().unwrap(), dedup[i], dedup[i + 1]);
        let cross = (p2.0 - p1.0) as i64 * (p3.1 - p1.1) as i64
            - (p2.1 - p1.1) as i64 * (p3.0 - p1.0) as i64;
        if cross.abs() > 25 {
            s.push(p2);
        }
    }
    s.push(*dedup.last().unwrap());
    if s.len() < 3 {
        return None;
    }

    let (xs, ys): (Vec<i32>, Vec<i32>) = s.iter().copied().unzip();
    let area: i64 = (0..s.len())
        .map(|i| {
            let (a, b) = (s[i], s[(i + 1) % s.len()]);
            a.0 as i64 * b.1 as i64 - b.0 as i64 * a.1 as i64
        })
        .sum();
    let span = |v: &[i32]| v.iter().max().unwrap() - v.iter().min().unwrap();
    if span(&xs) < 3 || span(&ys) < 3 || area.abs() < 40 {
        return None;
    }

    Some(s.into_iter().map(|(x, y)| (x as i16, y as i16)).collect())
}

pub fn gradient_layers(
    polys: &[Contour],
    slicing: Slicing,
    n_bands: u16,
) -> Vec<(u16, Vec<IntContour>)> {
    let (work, axis, undo) = match slicing {
        Slicing::Axis(a) => (polys.to_vec(), a, None),
        Slicing::Rotated(d) => (rotate(polys, d), Axis::Y, Some(-d)),
    };
    let Some((min, max)) = extent(&work, axis).filter(|(a, b)| b - a >= 1.0) else {
        return vec![];
    };
    let cross = extent(&work, axis.other()).unwrap();
    let step = (max - min) / n_bands as f64;

    (0..n_bands)
        .filter_map(|b| {
            let lo = min + b as f64 * step;
            let mut hi = min + (b + 1) as f64 * step;
            if b < n_bands - 1 {
                hi += 50.0;
            }

            let mut clipped = clip_band(&work, axis, lo, hi, cross);
            if let Some(d) = undo {
                clipped = rotate(&clipped, d);
            }

            let ints: Vec<_> = clipped.iter().filter_map(to_int_contour).collect();
            (!ints.is_empty()).then_some((b, ints))
        })
        .collect()
}
