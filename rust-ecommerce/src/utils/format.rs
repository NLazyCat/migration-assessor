pub fn format_price(amount: f64, currency: &str) -> String {
    format!("{} {:.2}", currency, amount)
}

pub fn format_date(date: &str) -> String {
    date.split('T').next().unwrap_or(date).to_string()
}

pub fn truncate_string(value: &str, max_length: usize) -> String {
    if value.len() <= max_length {
        value.to_string()
    } else {
        format!("{}...", &value[..max_length])
    }
}
