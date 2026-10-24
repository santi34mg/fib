//! Fixed-width scalar semantics shared by analysis and lowering, without LLVM.
use crate::frontend::tokens::{Literal, Operator as O, builtin::BuiltinType as B};
use num_traits::ToPrimitive;
use rustc_apfloat::{
    Float, FloatConvert, Round, Status,
    ieee::{Double, Half, Quad, Single},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericType {
    Integer { width: u16, signed: bool },
    Float(u16),
}
impl NumericType {
    pub fn builtin(b: &B) -> Option<Self> {
        use NumericType::*;
        Some(match b {
            B::Int8 => Integer {
                width: 8,
                signed: true,
            },
            B::Int16 => Integer {
                width: 16,
                signed: true,
            },
            B::Int32 => Integer {
                width: 32,
                signed: true,
            },
            B::Int64 => Integer {
                width: 64,
                signed: true,
            },
            B::Int128 => Integer {
                width: 128,
                signed: true,
            },
            B::UInt8 => Integer {
                width: 8,
                signed: false,
            },
            B::UInt16 => Integer {
                width: 16,
                signed: false,
            },
            B::UInt32 => Integer {
                width: 32,
                signed: false,
            },
            B::UInt64 => Integer {
                width: 64,
                signed: false,
            },
            B::UInt128 => Integer {
                width: 128,
                signed: false,
            },
            B::Float16 => Float(16),
            B::Float32 => Float(32),
            B::Float64 => Float(64),
            B::Float128 => Float(128),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericValue {
    pub ty: NumericType,
    pub bits: u128,
}
pub fn mask(width: u16) -> u128 {
    u128::MAX >> (128 - width)
}
pub fn signed(bits: u128, width: u16) -> i128 {
    ((bits << (128 - width)) as i128) >> (128 - width)
}

macro_rules! float_dispatch {
    ($width:expr, $function:ident $(, $arg:expr)*) => {
        match $width {
            16 => $function::<Half>($($arg),*),
            32 => $function::<Single>($($arg),*),
            64 => $function::<Double>($($arg),*),
            128 => $function::<Quad>($($arg),*),
            _ => unreachable!("fixed float width"),
        }
    };
}
fn parse_float<F: Float>(text: &str) -> anyhow::Result<u128> {
    let result = F::from_str_r(text, Round::NearestTiesToEven)
        .map_err(|e| anyhow::anyhow!("invalid floating literal: {e:?}"))?;
    anyhow::ensure!(
        !result.status.contains(Status::OVERFLOW),
        "floating-point literal is out of range"
    );
    Ok(result.value.to_bits())
}
fn int_float<F: Float>(bits: u128, width: u16, is_signed: bool) -> u128 {
    if is_signed {
        F::from_i128(signed(bits, width)).value.to_bits()
    } else {
        F::from_u128(bits).value.to_bits()
    }
}
fn float_int<F: Float>(bits: u128, width: u16, is_signed: bool) -> anyhow::Result<u128> {
    let value = F::from_bits(bits);
    let result = if is_signed {
        value
            .to_i128_r(usize::from(width), Round::TowardZero, &mut false)
            .map(|v| v as u128)
    } else {
        value.to_u128_r(usize::from(width), Round::TowardZero, &mut false)
    };
    anyhow::ensure!(
        !result.status.contains(Status::INVALID_OP),
        "float-to-integer cast value is out of range"
    );
    Ok(result.value & mask(width))
}
fn to_quad<F: Float + FloatConvert<Quad>>(bits: u128) -> Quad {
    F::from_bits(bits).convert(&mut false).value
}
fn from_quad<F: Float>(value: Quad) -> u128
where
    Quad: FloatConvert<F>,
{
    value.convert(&mut false).value.to_bits()
}
fn float_binary<F: Float>(a: u128, b: u128, op: O) -> anyhow::Result<u128> {
    let (a, b) = (F::from_bits(a), F::from_bits(b));
    Ok(match op {
        O::Plus => (a + b).value.to_bits(),
        O::Minus => (a - b).value.to_bits(),
        O::Star => (a * b).value.to_bits(),
        O::Slash => (a / b).value.to_bits(),
        _ => anyhow::bail!("invalid floating operator"),
    })
}
impl NumericValue {
    pub fn literal(literal: &Literal, ty: NumericType, negative: bool) -> anyhow::Result<Self> {
        let bits = match (literal, ty) {
            (Literal::Integer(magnitude), NumericType::Integer { width, signed }) => {
                let magnitude = magnitude
                    .to_u128()
                    .ok_or_else(|| anyhow::anyhow!("integer literal is out of range"))?;
                let fits = if negative {
                    signed && magnitude <= (1u128 << (width - 1))
                } else if signed {
                    magnitude < (1u128 << (width - 1))
                } else {
                    magnitude <= mask(width)
                };
                anyhow::ensure!(fits, "integer literal is out of range");
                (if negative {
                    magnitude.wrapping_neg()
                } else {
                    magnitude
                }) & mask(width)
            }
            (Literal::Float(text), NumericType::Float(width)) => {
                let text = if negative {
                    format!("-{text}")
                } else {
                    text.clone()
                };
                float_dispatch!(width, parse_float, &text)?
            }
            (Literal::Integer(magnitude), NumericType::Float(width)) => {
                let text = if negative {
                    format!("-{magnitude}")
                } else {
                    magnitude.to_string()
                };
                float_dispatch!(width, parse_float, &text)?
            }
            _ => anyhow::bail!("literal requires a matching numeric type"),
        };
        Ok(Self { ty, bits })
    }
    pub fn cast(self, ty: NumericType) -> anyhow::Result<Self> {
        use NumericType::*;
        let bits = match (self.ty, ty) {
            (Integer { width, signed: s }, Integer { width: dest, .. }) => {
                (if s {
                    signed(self.bits, width) as u128
                } else {
                    self.bits
                }) & mask(dest)
            }
            (Integer { width, signed }, Float(dest)) => {
                float_dispatch!(dest, int_float, self.bits, width, signed)
            }
            (
                Float(width),
                Integer {
                    width: dest,
                    signed,
                },
            ) => float_dispatch!(width, float_int, self.bits, dest, signed)?,
            (Float(width), Float(dest)) => {
                let quad = float_dispatch!(width, to_quad, self.bits);
                float_dispatch!(dest, from_quad, quad)
            }
        };
        Ok(Self { ty, bits })
    }
    pub fn unary(self, op: O) -> anyhow::Result<Self> {
        let bits = match self.ty {
            NumericType::Integer { width, .. } => match op {
                O::Minus => self.bits.wrapping_neg() & mask(width),
                O::Tilde => !self.bits & mask(width),
                _ => anyhow::bail!("invalid integer unary operator"),
            },
            NumericType::Float(width) if op == O::Minus => self.bits ^ (1u128 << (width - 1)),
            _ => anyhow::bail!("invalid numeric unary operator"),
        };
        Ok(Self { bits, ..self })
    }
    pub fn binary(self, rhs: Self, op: O) -> anyhow::Result<Self> {
        let bits = match self.ty {
            NumericType::Float(width) => {
                float_dispatch!(width, float_binary, self.bits, rhs.bits, op)?
            }
            NumericType::Integer { width, signed: s } => {
                let (a, b) = (self.bits, rhs.bits);
                let value = match op {
                    O::Plus => a.wrapping_add(b),
                    O::Minus => a.wrapping_sub(b),
                    O::Star => a.wrapping_mul(b),
                    O::Ampersand => a & b,
                    O::Pipe => a | b,
                    O::Caret => a ^ b,
                    O::Slash | O::Percent => {
                        anyhow::ensure!(b != 0, "constant division by zero");
                        if s {
                            let (a, b) = (signed(a, width), signed(b, width));
                            if op == O::Slash {
                                a.wrapping_div(b) as u128
                            } else {
                                a.wrapping_rem(b) as u128
                            }
                        } else if op == O::Slash {
                            a / b
                        } else {
                            a % b
                        }
                    }
                    O::LeftShift | O::RightShift => {
                        let negative = matches!(rhs.ty, NumericType::Integer { width, signed: true } if signed(b,width) < 0);
                        anyhow::ensure!(
                            !negative && b < u128::from(width),
                            "shift count is out of range"
                        );
                        if op == O::LeftShift {
                            a << b
                        } else if s {
                            (signed(a, width) >> b) as u128
                        } else {
                            a >> b
                        }
                    }
                    _ => anyhow::bail!("invalid integer binary operator"),
                };
                value & mask(width)
            }
        };
        Ok(Self { bits, ..self })
    }
    pub fn integer_i128(self) -> Option<i128> {
        match self.ty {
            NumericType::Integer {
                width,
                signed: true,
            } => Some(signed(self.bits, width)),
            NumericType::Integer { .. } => i128::try_from(self.bits).ok(),
            _ => None,
        }
    }
    pub fn is_zero(self) -> bool {
        match self.ty {
            NumericType::Float(w) => self.bits & mask(w - 1) == 0,
            _ => self.bits == 0,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    fn float(text: &str, width: u16) -> NumericValue {
        NumericValue::literal(
            &Literal::Float(text.into()),
            NumericType::Float(width),
            false,
        )
        .unwrap()
    }
    #[test]
    fn decimals_round_directly_to_the_destination_format() {
        assert_eq!(float("1.00048828125", 16).bits, 0x3c00);
        assert_eq!(float("1.00146484375", 16).bits, 0x3c02);
        assert_eq!(float("1.000000059604644775390625", 32).bits, 0x3f800000);
        // Rounding through f64 would discard the final digit and produce the wrong f32.
        assert_eq!(
            float("1.0000000596046447753906250000000000000000001", 32).bits,
            0x3f800001
        );
        assert_eq!(
            float("9007199254740993.0", 128).bits,
            ((16383u128 + 53) << 112) | (1 << 59)
        );
        let wide = float(&("1".to_owned() + &"0".repeat(400) + ".0"), 128);
        assert_ne!((wide.bits >> 112) & 0x7fff, 0x7fff);
    }
    #[test]
    fn subnormals_underflow_and_signed_zero_are_preserved() {
        assert_eq!(float("0.000000059604644775390625", 16).bits, 1);
        assert_eq!(float("0.0000000298023223876953125", 16).bits, 0);
        assert_eq!(float("-0.0", 128).bits, 1 << 127);
        let zero = float("0.0", 16);
        let infinity = float("1.0", 16).binary(zero, O::Slash).unwrap();
        assert_eq!(infinity.bits, 0x7c00);
        let nan = zero.binary(zero, O::Slash).unwrap();
        assert_eq!(nan.bits & 0x7c00, 0x7c00);
        assert_ne!(nan.bits & 0x3ff, 0);
    }
    #[test]
    fn full_integer_literals_and_wrapping_conversions() {
        let integer = |text: &str, width, signed, negative| {
            NumericValue::literal(
                &Literal::Integer(text.parse().unwrap()),
                NumericType::Integer { width, signed },
                negative,
            )
        };
        assert_eq!(
            integer(&u128::MAX.to_string(), 128, false, false)
                .unwrap()
                .bits,
            u128::MAX
        );
        assert_eq!(
            integer(&(1u128 << 127).to_string(), 128, true, true)
                .unwrap()
                .bits,
            1 << 127
        );
        assert!(integer(&(1u128 << 127).to_string(), 128, true, false).is_err());
        assert!(integer("340282366920938463463374607431768211456", 128, false, false).is_err());
        let neg = integer("1", 8, true, true).unwrap();
        assert_eq!(
            neg.cast(NumericType::Integer {
                width: 16,
                signed: false
            })
            .unwrap()
            .bits,
            65535
        );
        assert_eq!(
            float("-128.9", 64)
                .cast(NumericType::Integer {
                    width: 8,
                    signed: true
                })
                .unwrap()
                .bits,
            128
        );
        assert_eq!(
            float("-0.9", 64)
                .cast(NumericType::Integer {
                    width: 8,
                    signed: false
                })
                .unwrap()
                .bits,
            0
        );
        assert!(
            float("128.0", 64)
                .cast(NumericType::Integer {
                    width: 8,
                    signed: true
                })
                .is_err()
        );
    }
}
