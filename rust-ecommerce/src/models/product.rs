pub struct Product {
    pub id: u64,
    pub name: String,
    pub price: f64,
    pub category: String,
    pub in_stock: bool,
    pub tags: Vec<String>,
}

pub enum ProductStatus {
    Active,
    Discontinued,
    OutOfStock,
}

pub struct ProductFilters {
    pub category: Option<String>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub in_stock_only: Option<bool>,
}
