//! Saved logical touch-layout edits. Slots refer to stable authored controls.
use super::*;
use crate::persistence::{Document, SaveData, SaveError, Value};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEdit {
    pub offset: Vec2,
    pub scale: f32,
}
impl Default for TouchEdit {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchLayoutSettings {
    pub mirrored: bool,
    pub scale: f32,
    pub opacity: f32,
    pub edits: [TouchEdit; MAX_TOUCH_CONTROLS],
}
impl Default for TouchLayoutSettings {
    fn default() -> Self {
        Self {
            mirrored: false,
            scale: 1.,
            opacity: 0.65,
            edits: [TouchEdit::default(); MAX_TOUCH_CONTROLS],
        }
    }
}
impl TouchLayoutSettings {
    pub fn valid(self) -> bool {
        self.scale.is_finite()
            && (0.6..=1.8).contains(&self.scale)
            && self.opacity.is_finite()
            && (0.2..=1.).contains(&self.opacity)
            && self.edits.iter().all(|e| {
                e.offset.is_finite()
                    && e.offset.abs().max_element() <= 0.4
                    && e.scale.is_finite()
                    && (0.5..=2.).contains(&e.scale)
            })
    }
    pub fn apply(self, mut c: Control, slot: usize, viewport: Vec2) -> Result<Control, InputError> {
        if !self.valid()
            || slot >= MAX_TOUCH_CONTROLS
            || !c.valid()
            || !viewport.is_finite()
            || viewport.min_element() < 64.
        {
            return Err(InputError::InvalidTouch);
        }
        if self == Self::default() {
            return Ok(c);
        }
        if let Output::Look { .. } = c.output {
            if self.mirrored {
                c.region = match c.region {
                    Region::Rect(r) => {
                        Region::Rect(Rect::new(viewport.x - r.x - r.w, r.y, r.w, r.h))
                    }
                    Region::Circle { center, radius } => Region::Circle {
                        center: Vec2::new(viewport.x - center.x, center.y),
                        radius,
                    },
                };
            }
            return Ok(c);
        }
        let edit = self.edits[slot];
        let scale = self.scale * edit.scale;
        let mut center = c.region.center();
        if self.mirrored {
            center.x = viewport.x - center.x;
        }
        center += edit.offset * viewport;
        c.region = match c.region {
            Region::Circle { radius, .. } => {
                let radius = (radius * scale)
                    .max(22.)
                    .min(viewport.min_element() * 0.5 - 8.);
                let center = center.clamp(
                    Vec2::splat(radius + 8.),
                    viewport - Vec2::splat(radius + 8.),
                );
                Region::Circle { center, radius }
            }
            Region::Rect(r) => {
                let size = (Vec2::new(r.w, r.h) * scale)
                    .max(Vec2::splat(44.))
                    .min(viewport - Vec2::splat(16.));
                let half = size * 0.5;
                let center =
                    center.clamp(half + Vec2::splat(8.), viewport - half - Vec2::splat(8.));
                Region::Rect(Rect::new(
                    center.x - half.x,
                    center.y - half.y,
                    size.x,
                    size.y,
                ))
            }
        };
        if let Output::Stick { radius, .. } = &mut c.output {
            *radius = (*radius * scale).clamp(8., viewport.min_element() * 0.5 - 8.);
        }
        Ok(c)
    }
}
impl SaveData for TouchLayoutSettings {
    const SCHEMA: u64 = 0x4658544f55434831;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 512;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        if !self.valid() {
            return Err(SaveError::InvalidData);
        }
        let mut b = vec![u8::from(self.mirrored)];
        for v in [self.scale, self.opacity] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for e in self.edits {
            for v in [e.offset.x, e.offset.y, e.scale] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.set("layout", Value::Bytes(b))
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let b = d.bytes("layout")?;
        if b.len() != 201 || b[0] > 1 {
            return Err(SaveError::InvalidData);
        }
        let mut values = b[1..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| f32::from_le_bytes(*w));
        let mut result = Self {
            mirrored: b[0] != 0,
            scale: values.next().unwrap(),
            opacity: values.next().unwrap(),
            ..Default::default()
        };
        for e in &mut result.edits {
            *e = TouchEdit {
                offset: Vec2::new(values.next().unwrap(), values.next().unwrap()),
                scale: values.next().unwrap(),
            };
        }
        if !result.valid() {
            return Err(SaveError::InvalidData);
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_roundtrip_mirror_resize_and_stay_visible() {
        let mut p = TouchLayoutSettings {
            mirrored: true,
            scale: 1.5,
            ..Default::default()
        };
        p.edits[0].offset = Vec2::new(0.4, 0.4);
        assert_eq!(
            TouchLayoutSettings::decode(&p.encode(4).unwrap()).unwrap(),
            (p, 4)
        );
        let c = Control {
            region: Region::Circle {
                center: Vec2::new(40., 140.),
                radius: 20.,
            },
            output: Output::Button(2),
        };
        let output = p.apply(c, 0, Vec2::new(320., 200.)).unwrap();
        if let Region::Circle { center, radius } = output.region {
            assert!(center.x + radius <= 312. && center.y + radius <= 192.);
            assert!(radius >= 22.);
        }
        assert_eq!(output.output, c.output);
        assert!(p.apply(c, 16, Vec2::new(320., 200.)).is_err());
    }
    #[test]
    fn defaults_preserve_authored_controls() {
        let c = Control {
            region: Region::Rect(Rect::new(0., 0., 100., 100.)),
            output: Output::Look { x: 0, y: 1 },
        };
        assert_eq!(
            TouchLayoutSettings::default()
                .apply(c, 0, Vec2::new(320., 200.))
                .unwrap(),
            c
        );
    }
}
