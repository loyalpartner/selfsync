use axum::{Extension, response::Html};
use sea_orm::{DatabaseConnection, EntityTrait};

use crate::db::entity::user;

/// GET / — display current user list.
pub async fn list_users(Extension(db): Extension<DatabaseConnection>) -> Html<String> {
    let users = user::Entity::find().all(&db).await.unwrap_or_default();

    let mut html = String::from(
        "<html><head><title>selfsync</title>\
         <style>body{font-family:monospace;margin:2em}table{border-collapse:collapse}\
         td,th{border:1px solid #ccc;padding:6px 12px;text-align:left}</style>\
         </head><body><h2>selfsync users</h2>",
    );

    if users.is_empty() {
        html.push_str("<p>No users yet.</p>");
    } else {
        html.push_str(
            "<table><tr><th>ID</th><th>Email</th><th>Store Birthday</th><th>Next Version</th></tr>",
        );
        for u in &users {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                u.id, u.email, u.store_birthday, u.next_version
            ));
        }
        html.push_str("</table>");
    }

    html.push_str("</body></html>");
    Html(html)
}
