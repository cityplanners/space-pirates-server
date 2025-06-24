use crate::web::App;

mod users;
mod web;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    App::serve().await
}



