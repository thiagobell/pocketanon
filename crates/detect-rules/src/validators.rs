//! Checksum validators used by the Rule Detector. Pure functions over ASCII input.

/// IBAN check (ISO 13616, mod 97 = 1).
///
/// `iban` must be the compact form: no spaces, upper-case letters and ASCII digits, starting with
/// a two-letter country code and two check digits.
pub fn mod_97(iban: &str) -> bool {
    let b = iban.as_bytes();
    if b.len() < 5 || b.len() > 34 {
        return false;
    }
    if !b[0].is_ascii_uppercase() || !b[1].is_ascii_uppercase() || !b[2].is_ascii_digit() || !b[3].is_ascii_digit() {
        return false;
    }
    let mut rem: u32 = 0;
    for &c in b[4..].iter().chain(&b[..4]) {
        match c {
            b'0'..=b'9' => rem = (rem * 10 + u32::from(c - b'0')) % 97,
            b'A'..=b'Z' => rem = (rem * 100 + u32::from(c - b'A') + 10) % 97,
            _ => return false,
        }
    }
    rem == 1
}

/// Luhn check for payment card numbers. `digits` must be ASCII digits only.
pub fn luhn(digits: &str) -> bool {
    if digits.len() < 2 || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let sum: u32 = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, c)| {
            let d = u32::from(c - b'0');
            if i % 2 == 1 {
                let dd = d * 2;
                if dd > 9 { dd - 9 } else { dd }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// German tax identification number (Steuerliche Identifikationsnummer, IdNr), 11 digits.
///
/// Checks the structure rules (first digit not 0; among the first ten digits exactly one digit
/// occurs two or three times, all others at most once; a tripled digit may not appear three times
/// in a row) and the ISO 7064 MOD 11,10 check digit.
pub fn steuer_id(digits: &str) -> bool {
    let b = digits.as_bytes();
    if b.len() != 11 || !b.iter().all(u8::is_ascii_digit) || b[0] == b'0' {
        return false;
    }
    let d: Vec<u32> = b.iter().map(|c| u32::from(c - b'0')).collect();

    let mut counts = [0u8; 10];
    for &x in &d[..10] {
        counts[x as usize] += 1;
    }
    let repeated: Vec<usize> = (0..10).filter(|&i| counts[i] >= 2).collect();
    if repeated.len() != 1 {
        return false;
    }
    let r = repeated[0];
    if counts[r] > 3 {
        return false;
    }
    if counts[r] == 3 && d[..10].windows(3).any(|w| w.iter().all(|&x| x as usize == r)) {
        return false;
    }

    let mut product = 10;
    for &x in &d[..10] {
        let mut sum = (x + product) % 10;
        if sum == 0 {
            sum = 10;
        }
        product = (sum * 2) % 11;
    }
    let mut check = 11 - product;
    if check == 10 {
        check = 0;
    }
    check == d[10]
}

/// ICAO 9303 check digit (weights 7, 3, 1; `0`-`9` = 0-9, `A`-`Z` = 10-35), as used on German
/// identity cards and passports. Returns `None` for characters outside that alphabet.
pub fn icao_check_digit(s: &str) -> Option<u32> {
    const W: [u32; 3] = [7, 3, 1];
    let mut sum = 0;
    for (i, c) in s.bytes().enumerate() {
        let v = match c {
            b'0'..=b'9' => u32::from(c - b'0'),
            b'A'..=b'Z' => u32::from(c - b'A') + 10,
            _ => return None,
        };
        sum += v * W[i % 3];
    }
    Some(sum % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_97_accepts_valid_ibans() {
        assert!(mod_97("DE89370400440532013000"));
        assert!(mod_97("GB82WEST12345698765432"));
        assert!(mod_97("AT611904300234573201"));
        assert!(mod_97("NL91ABNA0417164300"));
    }

    #[test]
    fn mod_97_rejects_invalid_ibans() {
        assert!(!mod_97("DE89370400440532013001")); // altered last digit
        assert!(!mod_97("DE88370400440532013000")); // altered check digits
        assert!(!mod_97("DE8937040044053201300")); // too short for its value
        assert!(!mod_97("de89370400440532013000")); // lower case
        assert!(!mod_97("DE89 3704 0044 0532 0130 00")); // not compact
        assert!(!mod_97(""));
    }

    #[test]
    fn luhn_cases() {
        assert!(luhn("4111111111111111"));
        assert!(luhn("5555555555554444"));
        assert!(luhn("378282246310005"));
        assert!(luhn("6011000990139424"));
        assert!(!luhn("4111111111111112"));
        assert!(!luhn("1234567812345678"));
        assert!(!luhn("4111 1111 1111 1111"));
        assert!(!luhn("7"));
    }

    #[test]
    fn steuer_id_cases() {
        assert!(steuer_id("86095742719"));
        assert!(steuer_id("65929970489"));
        assert!(steuer_id("26954371827"));
        assert!(!steuer_id("86095742718")); // wrong check digit
        assert!(!steuer_id("06095742719")); // leading zero
        assert!(!steuer_id("12345678903")); // no repeated digit
        assert!(!steuer_id("11111111116")); // one digit repeated too often
        assert!(!steuer_id("8609574271")); // too short
        assert!(!steuer_id("8609574271a"));
    }

    #[test]
    fn icao_cases() {
        assert_eq!(icao_check_digit("T22000129"), Some(3));
        assert_eq!(icao_check_digit("T2200012?"), None);
    }
}
