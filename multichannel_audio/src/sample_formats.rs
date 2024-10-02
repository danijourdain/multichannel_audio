use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// `f64` sample format. Values have the range -1.0 to 1.0 with 0.0 being the origin.
    F32,

    // `i32` sample format. Values have the range i32::MIN to i32::MAX with 0 being the origin.
    I32,

    // `i16` sample format. Values have the range i16::MIN to i16::MAX with 0 being the origin.
    I16,

    // `i8` sample format. Values have the range i8::MIN to i8::MAX with 0 being the origin.
    I8,

    // `u32` sample format. Values have the range u64::MIN to u64::MAX with 0 being the origin.
    U32,

    // `u16` sample format. Values have the range u16::MIN to u16::MAX with 0 being the origin.
    U16,

    /// `u8` sample format. Values have the range u8::MIN to u8::MAX with 0 being the origin.
    U8,
}

impl fmt::Display for SampleFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SampleFormat::I8 => "i8",
                SampleFormat::I16 => "i16",
                SampleFormat::I32 => "i32",
                SampleFormat::U8 => "u8",
                SampleFormat::U16 => "u16",
                SampleFormat::U32 => "u32",
                SampleFormat::F32 => "f32",
            }
        )
    }
}
