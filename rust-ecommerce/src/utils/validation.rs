pub fn validate_email(email: &str) -> bool {
    email.contains('@') && email.contains('.')
}

pub fn validate_phone(phone: &str) -> bool {
    phone.len() >= 7 && phone.len() <= 15
}

pub fn is_non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}
