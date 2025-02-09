use sqlx::types::Uuid;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub current_room: Option<Uuid>,
}
