//! Bounded length expressions shared by computed style and layout.
//!
//! Font and viewport units become pixels at computed-value time. Percentages
//! retain their dependency on a containing size, including a zero percentage
//! in `calc()`. The parser never resolves percentages using the viewport.

use crate::style::Length;

pub const MAX_CALC_BYTES: usize = 4096;
pub const MAX_CALC_NESTING: usize = 32;
pub const MAX_CALC_TOKENS: usize = 1024;
pub const MAX_COMPONENTS: usize = 256;
const MAX_NUMBER: f64 = 16_384.0;
const MAX_COMPONENT: f64 = 16_777_216.0;

/// Static screen environment. Height is explicitly supplied, never inferred
/// from the eventual document extent. The width-only APIs leave it indefinite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: Option<f32>,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 900.0,
            height: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LengthContext {
    pub em: f32,
    pub rem: f32,
    pub viewport_width: f32,
    pub viewport_height: Option<f32>,
}

impl LengthContext {
    pub fn new(em: f32, rem: f32, viewport: Viewport) -> Self {
        Self {
            em,
            rem,
            viewport_width: viewport.width,
            viewport_height: viewport.height,
        }
    }
}

/// Split at top-level ASCII whitespace, keeping function arguments intact.
/// Unbalanced delimiters, excessive nesting, and excessive components reject
/// the complete shorthand rather than accepting a misleading prefix.
pub fn components(value: &str) -> Option<Vec<&str>> {
    split_components(value, None)
}

/// A bounded comma-list scanner for media, gradients, and shadows. Commas in
/// functions or quoted values do not split the list.
pub fn comma_components(value: &str) -> Option<Vec<&str>> {
    split_components(value, Some(','))
}

fn split_components(value: &str, separator: Option<char>) -> Option<Vec<&str>> {
    if value.len() > 65_536 {
        return None;
    }
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if Some(character) == quote {
            quote = None;
            continue;
        }
        if quote.is_some() {
            if character == '\n' {
                return None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            '(' => {
                depth += 1;
                if depth > MAX_CALC_NESTING {
                    return None;
                }
            }
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            _ if depth == 0
                && separator.map_or_else(
                    || character.is_ascii_whitespace(),
                    |separator| character == separator,
                ) =>
            {
                let component = value[start..index].trim();
                if !component.is_empty() || separator.is_some() {
                    result.push(component);
                    if result.len() > MAX_COMPONENTS {
                        return None;
                    }
                }
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 || quote.is_some() || escaped {
        return None;
    }
    let component = value[start..].trim();
    if !component.is_empty() || separator.is_some() {
        result.push(component);
    }
    (result.len() <= MAX_COMPONENTS).then_some(result)
}

#[derive(Clone, Copy)]
enum Value {
    Number(f64),
    Length {
        px: f64,
        percent: f64,
        percentage: bool,
    },
}

impl Value {
    fn checked(self) -> Option<Self> {
        let valid = |value: f64| value.is_finite() && value.abs() <= MAX_COMPONENT;
        match self {
            Self::Number(value) if valid(value) => Some(self),
            Self::Length { px, percent, .. } if valid(px) && valid(percent) => Some(self),
            _ => None,
        }
    }

    fn sum(self, other: Self, sign: f64) -> Option<Self> {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => Self::Number(a + sign * b).checked(),
            (
                Self::Length {
                    px: a,
                    percent: ap,
                    percentage: ah,
                },
                Self::Length {
                    px: b,
                    percent: bp,
                    percentage: bh,
                },
            ) => Self::Length {
                px: a + sign * b,
                percent: ap + sign * bp,
                percentage: ah || bh,
            }
            .checked(),
            _ => None,
        }
    }

    fn scale(self, factor: f64) -> Option<Self> {
        match self {
            Self::Number(value) => Self::Number(value * factor),
            Self::Length {
                px,
                percent,
                percentage,
            } => Self::Length {
                px: px * factor,
                percent: percent * factor,
                percentage,
            },
        }
        .checked()
    }

    fn product(self, other: Self, divide: bool) -> Option<Self> {
        match (self, other, divide) {
            (value, Self::Number(factor), false) => value.scale(factor),
            (Self::Number(factor), value, false) => value.scale(factor),
            (value, Self::Number(divisor), true) if divisor != 0.0 => value.scale(1.0 / divisor),
            _ => None,
        }
    }
}

struct Parser<'a> {
    source: &'a [u8],
    index: usize,
    depth: usize,
    tokens: usize,
    context: &'a LengthContext,
}

impl Parser<'_> {
    fn token(&mut self) -> Option<()> {
        self.tokens += 1;
        (self.tokens <= MAX_CALC_TOKENS).then_some(())
    }

    fn whitespace(&mut self) -> bool {
        let start = self.index;
        while self
            .source
            .get(self.index)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.index += 1;
        }
        start != self.index
    }

    fn sum(&mut self) -> Option<Value> {
        let mut value = self.product()?;
        loop {
            let whitespace_before = self.whitespace();
            let operator = self.source.get(self.index).copied();
            if !matches!(operator, Some(b'+') | Some(b'-')) {
                return Some(value);
            }
            // CSS requires whitespace on both sides of binary plus/minus.
            if !whitespace_before {
                return None;
            }
            self.token()?;
            self.index += 1;
            if !self.whitespace() {
                return None;
            }
            value = value.sum(
                self.product()?,
                if operator == Some(b'+') { 1.0 } else { -1.0 },
            )?;
        }
    }

    fn product(&mut self) -> Option<Value> {
        let mut value = self.atom()?;
        loop {
            // Leave whitespace for sum(), which checks the binary grammar.
            let before_whitespace = self.index;
            self.whitespace();
            let operator = self.source.get(self.index).copied();
            if !matches!(operator, Some(b'*') | Some(b'/')) {
                self.index = before_whitespace;
                return Some(value);
            }
            self.token()?;
            self.index += 1;
            value = value.product(self.atom()?, operator == Some(b'/'))?;
        }
    }

    fn atom(&mut self) -> Option<Value> {
        self.whitespace();
        self.token()?;
        let rest = self.source.get(self.index..)?;
        let parenthesis = rest.first() == Some(&b'(');
        let function = rest.len() >= 5 && rest[..5].eq_ignore_ascii_case(b"calc(");
        if parenthesis || function {
            self.depth += 1;
            if self.depth > MAX_CALC_NESTING {
                return None;
            }
            self.index += if function { 5 } else { 1 };
            let value = self.sum()?;
            self.whitespace();
            if self.source.get(self.index) != Some(&b')') {
                return None;
            }
            self.index += 1;
            self.depth -= 1;
            return Some(value);
        }
        self.number()
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.index;
        if matches!(self.source.get(self.index), Some(b'+') | Some(b'-')) {
            self.index += 1;
        }
        let digits = self.index;
        while self.source.get(self.index).is_some_and(u8::is_ascii_digit) {
            self.index += 1;
        }
        let mut digit_count = self.index - digits;
        if self.source.get(self.index) == Some(&b'.') {
            self.index += 1;
            let fractional = self.index;
            while self.source.get(self.index).is_some_and(u8::is_ascii_digit) {
                self.index += 1;
            }
            digit_count += self.index - fractional;
            if self.index == fractional {
                return None;
            }
        }
        if digit_count == 0 {
            return None;
        }
        if matches!(self.source.get(self.index), Some(b'e') | Some(b'E')) {
            let exponent = self.index;
            let mut cursor = self.index + 1;
            if matches!(self.source.get(cursor), Some(b'+') | Some(b'-')) {
                cursor += 1;
            }
            if self.source.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
                while self.source.get(cursor).is_some_and(u8::is_ascii_digit) {
                    cursor += 1;
                }
                self.index = cursor;
            } else {
                self.index = exponent;
            }
        }
        let number = std::str::from_utf8(&self.source[start..self.index])
            .ok()?
            .parse::<f64>()
            .ok()?;
        if !number.is_finite() || number.abs() > MAX_NUMBER {
            return None;
        }
        let unit_start = self.index;
        while self
            .source
            .get(self.index)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            self.index += 1;
        }
        if self.source.get(self.index) == Some(&b'%') {
            if self.index != unit_start {
                return None;
            }
            self.index += 1;
            return Some(Value::Length {
                px: 0.0,
                percent: number,
                percentage: true,
            });
        }
        let unit = std::str::from_utf8(&self.source[unit_start..self.index])
            .ok()?
            .to_ascii_lowercase();
        if unit.is_empty() {
            return Some(Value::Number(number));
        }
        let factor = match unit.as_str() {
            "px" => 1.0,
            "em" => f64::from(self.context.em),
            "rem" => f64::from(self.context.rem),
            "vw" => f64::from(self.context.viewport_width) / 100.0,
            "vh" => f64::from(self.context.viewport_height?) / 100.0,
            "vmin" => {
                f64::from(
                    self.context
                        .viewport_width
                        .min(self.context.viewport_height?),
                ) / 100.0
            }
            "vmax" => {
                f64::from(
                    self.context
                        .viewport_width
                        .max(self.context.viewport_height?),
                ) / 100.0
            }
            _ => return None,
        };
        Value::Length {
            px: number * factor,
            percent: 0.0,
            percentage: false,
        }
        .checked()
    }
}

pub fn parse_length(
    value: &str,
    context: &LengthContext,
    allow_auto: bool,
    allow_negative: bool,
) -> Option<Length> {
    let value = value.trim();
    if allow_auto && value.eq_ignore_ascii_case("auto") {
        return Some(Length::Auto);
    }
    if value.len() > MAX_CALC_BYTES || value.is_empty() || !value.is_ascii() {
        return None;
    }
    let calculation = value
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("calc("));
    let mut parser = Parser {
        source: value.as_bytes(),
        index: 0,
        depth: 0,
        tokens: 0,
        context,
    };
    let result = if calculation {
        parser.atom()?
    } else {
        parser.number()?
    };
    parser.whitespace();
    if parser.index != parser.source.len() {
        return None;
    }
    match result {
        Value::Number(0.0) if !calculation => Some(Length::Px(0.0)),
        Value::Number(_) => None,
        Value::Length {
            px,
            percent,
            percentage,
        } => {
            if !calculation && !allow_negative && (px < 0.0 || percent < 0.0) {
                return None;
            }
            if calculation && percentage {
                Some(Length::Calc {
                    px: px as f32,
                    percent: percent as f32,
                    percentage,
                    nonnegative: !allow_negative,
                })
            } else if percentage {
                Some(Length::Percent(percent as f32))
            } else {
                Some(Length::Px(if allow_negative {
                    px as f32
                } else {
                    px.max(0.0) as f32
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(height: Option<f32>) -> LengthContext {
        LengthContext::new(
            20.0,
            16.0,
            Viewport {
                width: 800.0,
                height,
            },
        )
    }

    #[test]
    fn lengths_resolve_font_and_explicit_viewport_units() {
        let context = context(Some(600.0));
        for (value, expected) in [
            ("2em", 40.0),
            ("2rem", 32.0),
            ("25vw", 200.0),
            ("50vh", 300.0),
            ("10vmin", 60.0),
            ("10vmax", 80.0),
            ("1e2px", 100.0),
        ] {
            assert_eq!(
                parse_length(value, &context, false, false),
                Some(Length::Px(expected))
            );
        }
        assert_eq!(
            parse_length("50vh", &self::context(None), false, false),
            None
        );
        assert_eq!(
            parse_length("10vmin", &self::context(None), false, false),
            None
        );
    }

    #[test]
    fn typed_calculation_preserves_percentage_dependency_and_precedence() {
        assert_eq!(
            parse_length(
                "calc((100% - 2em) / 2 + 3px * 2)",
                &context(None),
                false,
                false
            ),
            Some(Length::Calc {
                px: -14.0,
                percent: 50.0,
                percentage: true,
                nonnegative: true
            })
        );
        assert_eq!(
            parse_length("calc(10px + 0%)", &context(None), false, false),
            Some(Length::Calc {
                px: 10.0,
                percent: 0.0,
                percentage: true,
                nonnegative: true
            })
        );
        assert_eq!(
            parse_length("calc(2 * calc(1em + 4px))", &context(None), false, false),
            Some(Length::Px(48.0))
        );
        assert_eq!(
            parse_length("calc(2px - 8px)", &context(None), false, false),
            Some(Length::Px(0.0))
        );
        assert_eq!(
            parse_length("calc(2px - 8px)", &context(None), false, true),
            Some(Length::Px(-6.0))
        );
    }

    #[test]
    fn calculations_reject_invalid_types_syntax_and_nonfinite_values() {
        for value in [
            "calc(1px+2px)",
            "calc(1px -2px)",
            "calc(1px+ 2px)",
            "calc(1px * 2px)",
            "calc(1px / 2px)",
            "calc(1px / 0)",
            "calc(1px + 2)",
            "calc(0)",
            "calc(2px + red)",
            "calc(1px))",
            "1. px",
            "1 px",
            "NaNpx",
            "infpx",
            "1e300px",
            "16385px",
            "calc(16384px * 16384)",
            "min(1px, 2px)",
            "calc(-(1px))",
        ] {
            assert!(
                parse_length(value, &context(None), false, false).is_none(),
                "{value}"
            );
        }
    }

    #[test]
    fn calculation_and_component_limits_reject_complete_values() {
        let accepted = format!("calc({}1px{})", "(".repeat(31), ")".repeat(31));
        assert_eq!(
            parse_length(&accepted, &context(None), false, false),
            Some(Length::Px(1.0))
        );
        let deep = format!("calc({}1px{})", "(".repeat(32), ")".repeat(32));
        assert!(parse_length(&deep, &context(None), false, false).is_none());
        assert!(
            parse_length(
                &format!("calc({})", "1px + ".repeat(1000)),
                &context(None),
                false,
                false
            )
            .is_none()
        );
        assert!(
            components("1px calc(2px + 3px) 4px")
                .is_some_and(|parts| parts == ["1px", "calc(2px + 3px)", "4px"])
        );
        assert!(components("calc(1px + 2px").is_none());
        assert!(components(&"1px ".repeat(257)).is_none());
        assert_eq!(
            comma_components("rgba(1,2,3,0.5), calc(2px + 1px)"),
            Some(vec!["rgba(1,2,3,0.5)", "calc(2px + 1px)"])
        );
    }
}
