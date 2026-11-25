use std::fs;
use std::path::Path;

use nookery_model_data::{SOURCE_FILES, Value};

#[derive(Clone, Debug, PartialEq)]
enum Parsed {
    Integer(i128),
    Token(String),
    List(Vec<Parsed>),
}

fn strip_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"/*") {
            index += 2;
            while index + 1 < bytes.len() && !bytes[index..].starts_with(b"*/") {
                output.push(' ');
                index += 1;
            }
            index = (index + 2).min(bytes.len());
        } else if bytes[index..].starts_with(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                output.push(' ');
                index += 1;
            }
        } else {
            output.push(bytes[index] as char);
            index += 1;
        }
    }
    output
}

fn split_top_level(text: &str, delimiter: u8) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let (mut braces, mut brackets, mut parens) = (0, 0, 0);
    let mut quote = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote.is_some() {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if Some(byte) == quote {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else {
            match byte {
                b'{' => braces += 1,
                b'}' => braces -= 1,
                b'[' => brackets += 1,
                b']' => brackets -= 1,
                b'(' => parens += 1,
                b')' => parens -= 1,
                _ => {}
            }
            if byte == delimiter && braces == 0 && brackets == 0 && parens == 0 {
                parts.push(&text[start..index]);
                start = index + 1;
            }
        }
        index += 1;
    }
    parts.push(&text[start..]);
    parts
}

fn lexical_tokens(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let start = index;
        let byte = bytes[index];
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
        } else if byte == b'"' || byte == b'\'' {
            let quote = byte;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index += 2;
                } else if bytes[index] == quote {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
        } else {
            index += 1;
            if index < bytes.len() {
                let two = &bytes[start..=index];
                if matches!(
                    two,
                    b"<<"
                        | b">>"
                        | b"<="
                        | b">="
                        | b"=="
                        | b"!="
                        | b"&&"
                        | b"||"
                        | b"->"
                        | b"++"
                        | b"--"
                ) {
                    index += 1;
                }
            }
        }
        tokens.push(&text[start..index]);
    }
    tokens
}

fn canonical(text: &str) -> String {
    let mut result = String::new();
    for token in lexical_tokens(text.trim()) {
        if let (Some(previous), Some(current)) = (result.chars().last(), token.chars().next())
            && (previous.is_ascii_alphanumeric() || previous == '_')
            && (current.is_ascii_alphanumeric() || current == '_')
        {
            result.push(' ');
        }
        result.push_str(token);
    }
    result
}

fn parse_node(text: &str) -> Parsed {
    let text = text.trim();
    if text.starts_with('{') && text.ends_with('}') {
        return Parsed::List(
            split_top_level(&text[1..text.len() - 1], b',')
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .map(parse_node)
                .collect(),
        );
    }
    let token = canonical(text);
    let number = token.strip_prefix('-').unwrap_or(&token);
    let number = number.trim_end_matches(['u', 'U', 'l', 'L']);
    if !number.is_empty()
        && (number.chars().all(|character| character.is_ascii_digit())
            || number.starts_with("0x")
            || number.starts_with("0X"))
    {
        let radix = if number.starts_with("0x") || number.starts_with("0X") {
            16
        } else {
            10
        };
        let sign = if token.starts_with('-') { -1 } else { 1 };
        if let Ok(value) = i128::from_str_radix(
            number.trim_start_matches("0x").trim_start_matches("0X"),
            radix,
        ) {
            return Parsed::Integer(sign * value);
        }
    }
    Parsed::Token(token)
}

fn parse_source(source: &str) -> Vec<(String, String, Parsed)> {
    let source = strip_comments(source);
    let bytes = source.as_bytes();
    let mut declarations = Vec::new();
    let mut start = 0;
    let (mut braces, mut brackets, mut parens) = (0, 0, 0);
    for (index, byte) in bytes.iter().copied().enumerate() {
        match byte {
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            b'(' => parens += 1,
            b')' => parens -= 1,
            b';' if braces == 0 && brackets == 0 && parens == 0 => {
                let statement = &source[start..index];
                start = index + 1;
                let Some(equal) = statement.find('=') else {
                    continue;
                };
                let mut lhs = statement[..equal]
                    .lines()
                    .filter(|line| !line.trim_start().starts_with('#'))
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join(" ");
                while let Some(begin) = lhs.find("ATTRIBUTE_ALIGN(") {
                    let end = lhs[begin..].find(')').expect("alignment close") + begin + 1;
                    lhs.replace_range(begin..end, " ");
                }
                let lhs = lhs.trim();
                let lhs = lhs
                    .rfind('[')
                    .map_or(lhs, |array_start| lhs[..array_start].trim_end());
                let name = lhs.split_whitespace().last().expect("declaration name");
                let name_start = lhs.rfind(name).expect("declaration name offset");
                let name = name.to_owned();
                let c_type = lhs[..name_start]
                    .trim()
                    .trim_start_matches("static ")
                    .to_owned();
                let initializer = statement[equal + 1..].trim();
                let values = if initializer.starts_with('{') && initializer.ends_with('}') {
                    parse_node(initializer)
                } else {
                    Parsed::List(vec![parse_node(initializer)])
                };
                declarations.push((name, c_type, values));
            }
            _ => {}
        }
    }
    declarations
}

fn assert_values(expected: &[Value], actual: &Parsed, context: &str) {
    let Parsed::List(actual) = actual else {
        panic!("{context}: root is not a list")
    };
    assert_eq!(expected.len(), actual.len(), "{context}: element count");
    for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        let context = format!("{context}[{index}]");
        match (expected, actual) {
            (Value::Integer(expected), Parsed::Integer(actual)) => {
                assert_eq!(*expected, *actual, "{context}")
            }
            (Value::Token(expected), Parsed::Token(actual)) => {
                assert_eq!(*expected, actual, "{context}")
            }
            (Value::List(expected), Parsed::List(actual)) => {
                assert_values(expected, &Parsed::List(actual.clone()), &context)
            }
            (expected, actual) => panic!("{context}: generated {expected:?}, C {actual:?}"),
        }
    }
}

#[test]
fn every_eligible_source_matches_independent_reader_element_by_element() {
    assert_eq!(SOURCE_FILES.len(), 192);
    for source in SOURCE_FILES {
        let bytes = fs::read(repo_root().join(source.source_path)).expect("C source");
        assert_eq!(
            bytes.len(),
            source.source_bytes,
            "{} bytes",
            source.source_path
        );
        assert_eq!(
            sha256_hex(&bytes),
            source.source_sha256,
            "{} hash",
            source.source_path
        );
        let parsed = parse_source(std::str::from_utf8(&bytes).expect("UTF-8 C source"));
        assert_eq!(
            parsed.len(),
            source.declarations.len(),
            "{} declarations",
            source.source_path
        );
        for ((name, c_type, values), declaration) in parsed.iter().zip(source.declarations) {
            assert_eq!(name, declaration.name, "{} symbol", source.source_path);
            assert_eq!(c_type, declaration.c_type, "{} type", source.source_path);
            assert_values(
                declaration.values,
                values,
                &format!("{}::{name}", source.source_path),
            );
        }
    }
}

#[test]
fn generated_manifest_is_complete_and_excludes_disc_payloads() {
    for source in SOURCE_FILES {
        let text = fs::read_to_string(repo_root().join(source.source_path)).expect("C source");
        assert!(
            !text.contains("#include \"assets/"),
            "disc payload: {}",
            source.source_path
        );
    }
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repository root")
}

fn sha256_hex(input: &[u8]) -> String {
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64) * 8;
    let mut data = input.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    for chunk in data.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (word, bytes) in schedule[..16].iter_mut().zip(chunk.chunks_exact(4)) {
            *word = u32::from_be_bytes(bytes.try_into().expect("word"));
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }
        let mut working = state;
        for index in 0..64 {
            let s1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let choice = (working[4] & working[5]) ^ ((!working[4]) & working[6]);
            let temp1 = working[7]
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let s0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let majority =
                (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let temp2 = s0.wrapping_add(majority);
            working = [
                temp1.wrapping_add(temp2),
                working[0],
                working[1],
                working[2],
                working[3].wrapping_add(temp1),
                working[4],
                working[5],
                working[6],
            ];
        }
        for (word, value) in state.iter_mut().zip(working) {
            *word = word.wrapping_add(value);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}
