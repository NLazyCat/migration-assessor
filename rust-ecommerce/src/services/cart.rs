pub struct CartItem {
    pub product: Product,
    pub quantity: u32,
}

pub struct CartService {
    items: std::collections::HashMap<u64, Vec<CartItem>>,
}

impl CartService {
    pub fn new() -> Self {
        Self { items: std::collections::HashMap::new() }
    }

    pub async fn add_item(&mut self, user_id: u64, product: Product, quantity: u32) -> CartItem {
        let user_items = self.items.entry(user_id).or_default();
        let existing = user_items.iter_mut().find(|item| item.product.id == product.id);
        match existing {
            Some(item) => item.quantity += quantity,
            None => user_items.push(CartItem { product, quantity }),
        }
        user_items.last().unwrap().clone()
    }

    pub async fn remove_item(&mut self, user_id: u64, product_id: u64) -> bool {
        let Some(user_items) = self.items.get_mut(&user_id) else { return false };
        let index = user_items.iter().position(|item| item.product.id == product_id);
        match index {
            Some(i) => { user_items.remove(i); true }
            None => false,
        }
    }

    pub async fn get_total(&self, user_id: u64) -> f64 {
        let Some(user_items) = self.items.get(&user_id) else { return 0.0 };
        user_items.iter().map(|item| item.product.price * item.quantity as f64).sum()
    }

    pub async fn clear_cart(&mut self, user_id: u64) {
        self.items.remove(&user_id);
    }

    pub async fn get_item_count(&self, user_id: u64) -> u32 {
        let Some(user_items) = self.items.get(&user_id) else { return 0 };
        user_items.iter().map(|item| item.quantity).sum()
    }
}
