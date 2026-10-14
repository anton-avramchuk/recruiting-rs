use platform_values::{Email, Phone, ValueError};

#[test]
fn email_is_trimmed_and_lowercased() {
    assert_eq!(
        Email::parse("  Anna@Acme.IO ").unwrap().as_str(),
        "anna@acme.io"
    );
}

#[test]
fn invalid_emails_are_rejected() {
    for bad in ["", "no-at", "@x.io", "a@b", "a b@x.io", "a@.io", "a@x."] {
        assert_eq!(Email::parse(bad), Err(ValueError::InvalidEmail), "{bad}");
    }
}

#[test]
fn different_spellings_of_one_phone_are_equal() {
    let variants = [
        "8 (916) 123-45-67",
        "+7 916 123 45 67",
        "+7(916)123-45-67",
        "79161234567",
        "8-916-123-45-67",
    ];
    let canon: Vec<_> = variants.iter().map(|v| Phone::parse(v).unwrap()).collect();
    for p in &canon {
        assert_eq!(p.as_str(), "+79161234567");
        assert_eq!(p, &canon[0]);
    }
}

#[test]
fn foreign_numbers_keep_their_country_code() {
    assert_eq!(
        Phone::parse("+49 151 2345 6789").unwrap().as_str(),
        "+4915123456789"
    );
}

#[test]
fn phone_needs_at_least_ten_digits() {
    for bad in [
        "",
        "12345",
        "+7 916",
        "abc",
        "123456789",
        "+1234567890123456",
    ] {
        assert_eq!(Phone::parse(bad), Err(ValueError::InvalidPhone), "{bad}");
    }
}

#[test]
fn significant_digits_are_the_last_ten() {
    let a = Phone::parse("8 (916) 123-45-67").unwrap();
    let b = Phone::parse("+7 916 123 45 67").unwrap();
    assert_eq!(a.significant_digits(), "9161234567");
    assert_eq!(a.significant_digits(), b.significant_digits());
}

#[test]
fn values_serialize_as_plain_strings() {
    assert_eq!(
        serde_json::to_string(&Email::parse("A@b.io").unwrap()).unwrap(),
        "\"a@b.io\""
    );
    assert_eq!(
        serde_json::to_string(&Phone::parse("8 916 123 45 67").unwrap()).unwrap(),
        "\"+79161234567\""
    );
}
