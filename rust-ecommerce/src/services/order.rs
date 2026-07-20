pub struct Order {
    pub id: u64,
    pub user_id: u64,
    pub items: Vec<OrderItem>,
    pub total_amount: f64,
    pub status: OrderStatus,
    pub created_at: String,
}

pub struct OrderItem {
    pub product: crate::models::product::Product,
    pub quantity: u32,
}

pub enum OrderStatus {
    Pending,
    Confirmed,
    Shipped,
    Delivered,
    Cancelled,
}

pub struct OrderService {
    orders: std::collections::HashMap<u64, Order>,
}

impl OrderService {
    pub fn new() -> Self {
        Self { orders: std::collections::HashMap::new() }
    }

    pub async fn create_order(
        &mut self,
        user: &crate::models::user::User,
        items: Vec<OrderItem>,
    ) -> Order {
        let total_amount = items.iter().map(|i| i.product.price * i.quantity as f64).sum();
        let id = self.orders.len() as u64 + 1;
        let order = Order {
            id,
            user_id: user.id,
            items,
            total_amount,
            status: OrderStatus::Pending,
            created_at: String::new(),
        };
        self.orders.insert(order.id, order);
        self.orders.get(&id).unwrap().clone()
    }

    pub async fn get_order_by_id(&self, order_id: u64) -> Option<&Order> {
        self.orders.get(&order_id)
    }

    pub async fn list_orders_by_user(&self, user_id: u64) -> Vec<&Order> {
        self.orders.values().filter(|o| o.user_id == user_id).collect()
    }

    pub async fn update_order_status(&mut self, order_id: u64, status: OrderStatus) -> bool {
        let Some(order) = self.orders.get_mut(&order_id) else { return false };
        order.status = status;
        true
    }
}
