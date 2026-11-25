//! Source-compatible formatting helpers for sprintf.c and aprintf.c.
//!
//! Rust cannot define a stable C-variadic function, so this module exposes the
//! same formatting behavior through a typed argument list. The parser covers
//! the conversion family used by the PC vsnprintf implementation and keeps
//! callback delivery and return-count behavior explicit.

use std::ffi::{c_char, c_void};

/// One typed value accepted by the host formatting shim.
#[derive(Clone, Debug, PartialEq)]
pub enum FormatArg<'a> {
    /// Signed decimal integer.
    Signed(i64),
    /// Unsigned integer.
    Unsigned(u64),
    /// Floating-point value.
    Float(f64),
    /// String value.
    String(&'a str),
    /// Single character value.
    Character(char),
}

/// A callback target matching aprout_func_t's data shape.
#[repr(C)]
pub struct AproutFunc {
    /// Callback invoked with one complete formatted result.
    pub prout_func: Option<unsafe extern "C" fn(*mut c_void, *const c_char, i32) -> *mut c_void>,
    /// Opaque callback context.
    pub context: *mut c_void,
}

/// Format a typed argument list into a Rust string.
pub fn format_string(fmt: &str, args: &[FormatArg<'_>]) -> Result<String, FormatError> {
    let mut output = String::new();
    let mut chars = fmt.chars().peekable();
    let mut index = 0;
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            output.push('%');
            continue;
        }

        let mut zero_pad = false;
        let mut left = false;
        let mut width = 0usize;
        let mut precision = None;
        loop {
            match chars.peek().copied() {
                Some('0') if width == 0 => {
                    zero_pad = true;
                    chars.next();
                }
                Some('-') => {
                    left = true;
                    chars.next();
                }
                Some(c) if c.is_ascii_digit() => {
                    width = width * 10 + c.to_digit(10).unwrap() as usize;
                    chars.next();
                }
                _ => break,
            }
        }
        if chars.peek() == Some(&'.') {
            chars.next();
            let mut value = 0usize;
            while let Some(c) = chars.peek().copied() {
                if let Some(digit) = c.to_digit(10) {
                    value = value * 10 + digit as usize;
                    chars.next();
                } else {
                    break;
                }
            }
            precision = Some(value);
        }
        while matches!(chars.peek(), Some('h' | 'l' | 'L' | 'z' | 'j' | 't')) {
            chars.next();
        }
        let conversion = chars.next().ok_or(FormatError::IncompleteConversion)?;
        let arg = args.get(index).ok_or(FormatError::MissingArgument)?;
        index += 1;
        let rendered = match (conversion, arg) {
            ('d' | 'i', FormatArg::Signed(value)) => value.to_string(),
            ('u', FormatArg::Unsigned(value)) => value.to_string(),
            ('x', FormatArg::Unsigned(value)) => format!("{value:x}"),
            ('X', FormatArg::Unsigned(value)) => format!("{value:X}"),
            ('f', FormatArg::Float(value)) => {
                format!("{:.*}", precision.unwrap_or(6), value)
            }
            ('s', FormatArg::String(value)) => (*value).to_owned(),
            ('c', FormatArg::Character(value)) => value.to_string(),
            _ => return Err(FormatError::ArgumentTypeMismatch),
        };
        append_padded(&mut output, &rendered, width, zero_pad, left);
    }
    Ok(output)
}

/// Errors produced by the typed formatting shim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatError {
    /// A conversion marker was the final format character.
    IncompleteConversion,
    /// The format required more arguments than were supplied.
    MissingArgument,
    /// The argument variant did not match the conversion.
    ArgumentTypeMismatch,
    /// The output was too large for a signed C int return count.
    OutputTooLong,
}

/// Format into a NUL-terminated destination buffer, returning the source count.
pub fn sprintf(dst: &mut [u8], fmt: &str, args: &[FormatArg<'_>]) -> Result<i32, FormatError> {
    let rendered = format_string(fmt, args)?;
    let count = i32::try_from(rendered.len()).map_err(|_| FormatError::OutputTooLong)?;
    if dst.len() <= rendered.len() {
        return Err(FormatError::OutputTooLong);
    }
    dst[..rendered.len()].copy_from_slice(rendered.as_bytes());
    dst[rendered.len()] = 0;
    Ok(count)
}

/// Apply the TARGET_PC vaprintf callback contract to typed arguments.
pub fn vaprintf(func: &AproutFunc, fmt: &str, args: &[FormatArg<'_>]) -> Result<i32, FormatError> {
    let rendered = format_string(fmt, args)?;
    let count = i32::try_from(rendered.len()).map_err(|_| FormatError::OutputTooLong)?;
    let callback = func.prout_func.ok_or(FormatError::ArgumentTypeMismatch)?;
    let mut bytes = rendered.into_bytes();
    // The C callback receives a non-NUL-terminated byte span of written bytes.
    bytes.push(0);
    // SAFETY: the callback is supplied by the caller and receives a valid
    // pointer for exactly the source-reported byte count.
    let result = unsafe { callback(func.context, bytes.as_ptr() as *const c_char, count) };
    if result.is_null() {
        return Err(FormatError::ArgumentTypeMismatch);
    }
    Ok(count)
}

fn append_padded(output: &mut String, value: &str, width: usize, zero_pad: bool, left: bool) {
    let padding = width.saturating_sub(value.len());
    if left {
        output.push_str(value);
        output.extend(std::iter::repeat_n(' ', padding));
    } else if zero_pad && value.starts_with('-') {
        output.push('-');
        output.extend(std::iter::repeat_n('0', padding));
        output.push_str(&value[1..]);
    } else {
        let fill = if zero_pad { '0' } else { ' ' };
        output.extend(std::iter::repeat_n(fill, padding));
        output.push_str(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprintf_keeps_c_count_and_nul_termination() {
        let mut dst = [0; 32];
        let count = sprintf(
            &mut dst,
            "%s:%04x",
            &[FormatArg::String("oak"), FormatArg::Unsigned(0x2a)],
        )
        .unwrap();
        assert_eq!(count, 8);
        assert_eq!(&dst[..9], b"oak:002a\0");
    }
}
