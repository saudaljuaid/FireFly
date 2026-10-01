//! Shared, horizontal-writing sizing vocabulary. Numeric algorithms never paint.
use crate::style::{BoxSizing, Length};

pub const MAX_SIZE: f32 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvailableSize {
    Definite(f32),
    Indefinite,
    MinContent,
    MaxContent,
}

impl AvailableSize {
    pub fn definite(self) -> Option<f32> {
        match self {
            Self::Definite(value) if value.is_finite() => Some(value.clamp(0.0, MAX_SIZE)),
            _ => None,
        }
    }
    pub fn from_option(value: Option<f32>) -> Self {
        value.map_or(Self::Indefinite, Self::Definite)
    }
}

/// A used min/max constraint can supply alignment space without making an
/// automatic containing size definite for percentage resolution.
#[derive(Debug, Clone, Copy)]
pub struct AxisSpace {
    pub available: AvailableSize,
    pub percentage_basis: Option<f32>,
}

impl AxisSpace {
    pub fn new(used: Option<f32>, definite: Option<f32>) -> Self {
        Self {
            available: AvailableSize::from_option(used),
            percentage_basis: AvailableSize::from_option(definite).definite(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IntrinsicSizes {
    /// Content sizes. Contributions add padding, borders, and non-auto margins.
    pub min_content: f32,
    pub max_content: f32,
}

impl IntrinsicSizes {
    pub fn new(min_content: f32, max_content: f32) -> Self {
        let min_content = dimension(min_content);
        Self {
            min_content,
            max_content: dimension(max_content).max(min_content),
        }
    }
    pub fn resolve(self, value: Length, containing: AvailableSize) -> Option<f32> {
        match value {
            Length::MinContent => Some(self.min_content),
            Length::MaxContent => Some(self.max_content),
            _ => value
                .resolve_indefinite(containing.definite())
                .map(dimension),
        }
    }
    pub fn shrink_to_fit(self, available: f32) -> f32 {
        dimension(available)
            .max(self.min_content)
            .min(self.max_content)
    }
}

pub fn dimension(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, MAX_SIZE)
    } else {
        0.0
    }
}

pub fn content_size(value: f32, inset: f32, box_sizing: BoxSizing) -> f32 {
    dimension(
        value
            - if box_sizing == BoxSizing::BorderBox {
                inset
            } else {
                0.0
            },
    )
}

pub fn resolve_content_size(
    intrinsic: IntrinsicSizes,
    length: Length,
    available: AvailableSize,
    inset: f32,
    box_sizing: BoxSizing,
) -> Option<f32> {
    intrinsic.resolve(length, available).map(|value| {
        if matches!(length, Length::MinContent | Length::MaxContent) {
            value
        } else {
            content_size(value, inset, box_sizing)
        }
    })
}

/// Min wins over max; both constraints refer to the same CSS sizing box.
pub fn constrain(value: f32, minimum: Option<f32>, maximum: Option<f32>) -> f32 {
    dimension(
        value
            .min(maximum.unwrap_or(MAX_SIZE))
            .max(minimum.unwrap_or(0.0)),
    )
}

/// CSS 2.2 §§10.3.2/10.4 replaced sizing. One specified axis transfers its
/// constrained used size through the intrinsic ratio. Two auto axes retain the
/// ratio when min/max constraints permit it; opposing constraints can override
/// it. All dimensions use the content box.
pub fn replaced_size(
    intrinsic: (f32, f32),
    preferred: (Option<f32>, Option<f32>),
    minimum: (Option<f32>, Option<f32>),
    maximum: (Option<f32>, Option<f32>),
) -> (f32, f32) {
    let (iw, ih) = (dimension(intrinsic.0), dimension(intrinsic.1));
    let (min_w, min_h) = (
        minimum.0.map(dimension).unwrap_or(0.0),
        minimum.1.map(dimension).unwrap_or(0.0),
    );
    let (max_w, max_h) = (
        maximum.0.map(dimension).unwrap_or(MAX_SIZE).max(min_w),
        maximum.1.map(dimension).unwrap_or(MAX_SIZE).max(min_h),
    );
    let width_ratio = |h: f32| if ih > 0.0 { dimension(iw * h / ih) } else { iw };
    let height_ratio = |w: f32| if iw > 0.0 { dimension(ih * w / iw) } else { ih };
    match preferred {
        (Some(w), Some(h)) => (
            dimension(w).clamp(min_w, max_w),
            dimension(h).clamp(min_h, max_h),
        ),
        (Some(w), None) => {
            let w = dimension(w).clamp(min_w, max_w);
            (w, height_ratio(w).clamp(min_h, max_h))
        }
        (None, Some(h)) => {
            let h = dimension(h).clamp(min_h, max_h);
            (width_ratio(h).clamp(min_w, max_w), h)
        }
        (None, None) => {
            if iw == 0.0 || ih == 0.0 {
                return (iw.clamp(min_w, max_w), ih.clamp(min_h, max_h));
            }
            if iw > max_w && ih < min_h {
                return (max_w, min_h);
            }
            if iw < min_w && ih > max_h {
                return (min_w, max_h);
            }
            let scale = if iw > max_w || ih > max_h {
                (max_w / iw).min(max_h / ih).min(1.0)
            } else if iw < min_w || ih < min_h {
                (min_w / iw).max(min_h / ih).max(1.0)
            } else {
                1.0
            };
            (
                dimension(iw * scale).clamp(min_w, max_w),
                dimension(ih * scale).clamp(min_h, max_h),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indefinite_percentages_and_constraint_precedence_are_explicit() {
        assert_eq!(Length::Percent(50.0).resolve_indefinite(None), None);
        assert_eq!(
            Length::Percent(50.0).resolve_indefinite(Some(200.0)),
            Some(100.0)
        );
        assert_eq!(content_size(30.0, 40.0, BoxSizing::BorderBox), 0.0);
        assert_eq!(constrain(80.0, Some(100.0), Some(50.0)), 100.0);
        assert_eq!(AvailableSize::Definite(f32::INFINITY).definite(), None);
    }
    #[test]
    fn replaced_constraints_preserve_ratio_or_resolve_opposing_limits() {
        let resolve =
            |preferred, minimum, maximum| replaced_size((128.0, 64.0), preferred, minimum, maximum);
        assert_eq!(
            resolve((None, None), (None, None), (None, Some(20.0))),
            (40.0, 20.0)
        );
        assert_eq!(
            resolve((None, None), (None, Some(100.0)), (None, None)),
            (200.0, 100.0)
        );
        assert_eq!(
            resolve((None, None), (None, None), (Some(60.0), Some(20.0))),
            (40.0, 20.0)
        );
        assert_eq!(
            resolve((None, None), (Some(100.0), None), (None, Some(20.0))),
            (100.0, 20.0)
        );
        assert_eq!(
            resolve((None, Some(80.0)), (None, None), (None, Some(40.0))),
            (80.0, 40.0)
        );
        assert_eq!(
            resolve((Some(80.0), None), (Some(160.0), None), (None, None)),
            (160.0, 80.0)
        );
        assert_eq!(
            resolve(
                (None, None),
                (Some(200.0), Some(100.0)),
                (Some(50.0), Some(20.0))
            ),
            (200.0, 100.0)
        );
    }
}
