//! Cheap palette transforms shared by the GPU and software examples.
use crate::Color;
/// Authored keeps original colors. Presets preserve alpha and need no extra pass.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    #[default]
    Authored = 0,
    Warm = 1,
    Cool = 2,
    Ink = 3,
}
impl Look {
    pub fn from_index(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Authored),
            1 => Some(Self::Warm),
            2 => Some(Self::Cool),
            3 => Some(Self::Ink),
            _ => None,
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "authored" => Some(Self::Authored),
            "warm" => Some(Self::Warm),
            "cool" => Some(Self::Cool),
            "ink" => Some(Self::Ink),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Authored => "Authored",
            Self::Warm => "Warm",
            Self::Cool => "Cool",
            Self::Ink => "Ink",
        }
    }
    pub fn shade(self, color: Color) -> Color {
        let Color { r, g, b, a } = color;
        match self {
            Self::Authored => color,
            Self::Warm => Color::new((r * 1.06 + g * 0.02).min(1.), g * 0.98, b * 0.88, a),
            Self::Cool => Color::new(
                r * 0.87,
                (g * 1.02).min(1.),
                (b * 1.08 + r * 0.015).min(1.),
                a,
            ),
            Self::Ink => {
                let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
                Color::new(y * 0.82, y * 0.94, y, a)
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_preserve_alpha_and_authored_identity() {
        for color in [
            Color::new(0.9, 0.4, 0.2, 0.3),
            Color::new(0., 0., 0., 0.),
            Color::new(1., 1., 1., 1.),
        ] {
            assert_eq!(Look::Authored.shade(color), color);
            for i in 0..4 {
                let look = Look::from_index(i).unwrap();
                let c = look.shade(color);
                assert_eq!(c.a, color.a);
                assert!(c.to_array().iter().all(|v| (0. ..=1.).contains(v)));
                assert_eq!(Look::from_name(&look.name().to_lowercase()), Some(look));
            }
        }
        assert_eq!(Look::from_index(4), None);
    }
}
