//! Exact finite decimal comparison without exponent expansion or floating point.
use std::cmp::Ordering;
// Signed decimal integer arithmetic is used only to adjust a decimal exponent by
// the bounded coefficient length. Its storage is proportional to input length.
fn normalized(s: &str) -> (bool, String) {
    let negative = s.starts_with('-');
    let digits = s.trim_start_matches(['-', '+']).trim_start_matches('0');
    (
        negative && !digits.is_empty(),
        if digits.is_empty() {
            "0".into()
        } else {
            digits.into()
        },
    )
}
fn magnitude(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}
fn sum(a: &str, b: &str) -> String {
    let mut out = Vec::new();
    let mut carry = 0;
    let mut aa = a.bytes().rev();
    let mut bb = b.bytes().rev();
    loop {
        let x = aa.next();
        let y = bb.next();
        if x.is_none() && y.is_none() && carry == 0 {
            break;
        }
        let n = x.map_or(0, |v| v - b'0') + y.map_or(0, |v| v - b'0') + carry;
        out.push(b'0' + n % 10);
        carry = n / 10;
    }
    out.reverse();
    String::from_utf8(out).expect("decimal digits")
}
fn difference(a: &str, b: &str) -> String {
    let mut out = Vec::new();
    let mut borrow = 0i16;
    let mut bb = b.bytes().rev();
    for x in a.bytes().rev() {
        let n = i16::from(x - b'0') - bb.next().map_or(0, |v| i16::from(v - b'0')) - borrow;
        borrow = i16::from(n < 0);
        out.push(b'0' + n.rem_euclid(10) as u8);
    }
    out.reverse();
    normalized(std::str::from_utf8(&out).expect("decimal digits")).1
}
fn add(a: (bool, String), b: (bool, String)) -> (bool, String) {
    if a.0 == b.0 {
        return (a.0, sum(&a.1, &b.1));
    }
    match magnitude(&a.1, &b.1) {
        Ordering::Greater => (a.0, difference(&a.1, &b.1)),
        Ordering::Less => (b.0, difference(&b.1, &a.1)),
        Ordering::Equal => (false, "0".into()),
    }
}
fn signed_cmp(a: &(bool, String), b: &(bool, String)) -> Ordering {
    match (a.0, b.0) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => {
            let order = magnitude(&a.1, &b.1);
            if a.0 { order.reverse() } else { order }
        }
    }
}
struct Decimal {
    negative: bool,
    coefficient: String,
    position: (bool, String),
}
impl Decimal {
    fn parse(text: &str) -> Self {
        let negative = text.starts_with('-');
        let text = text.trim_start_matches('-');
        let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let coefficient = format!("{whole}{fraction}")
            .trim_start_matches('0')
            .to_string();
        let adjustment = coefficient.len() as i64 - fraction.len() as i64;
        Self {
            negative: negative && !coefficient.is_empty(),
            coefficient,
            position: add(normalized(exponent), normalized(&adjustment.to_string())),
        }
    }
}
/// Inputs must already satisfy the declared integer or decimal schema.
pub(super) fn compare(a: &str, b: &str) -> Ordering {
    let a = Decimal::parse(a);
    let b = Decimal::parse(b);
    if a.coefficient.is_empty() && b.coefficient.is_empty() {
        return Ordering::Equal;
    }
    if a.negative != b.negative {
        return if a.negative {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let order = if a.coefficient.is_empty() {
        Ordering::Less
    } else if b.coefficient.is_empty() {
        Ordering::Greater
    } else {
        signed_cmp(&a.position, &b.position).then_with(|| {
            let length = a.coefficient.len().max(b.coefficient.len());
            a.coefficient
                .bytes()
                .chain(std::iter::repeat(b'0'))
                .take(length)
                .cmp(
                    b.coefficient
                        .bytes()
                        .chain(std::iter::repeat(b'0'))
                        .take(length),
                )
        })
    };
    if a.negative { order.reverse() } else { order }
}
