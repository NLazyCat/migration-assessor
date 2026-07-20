pub struct Address {
    pub street: String,
    pub city: String,
    pub zip_code: String,
    pub country: String,
}

pub struct User {
    pub id: u64,
    pub display_name: String,
    pub email: String,
    pub address: Address,
    pub created_at: String,
}

pub enum UserRole {
    Admin,
    User,
    Viewer,
}
