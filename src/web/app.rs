use axum_login::{
    login_required,
    AuthManagerLayerBuilder,
    tower_sessions::SessionManagerLayer,
};
use axum_messages::MessagesManagerLayer;
use time::Duration;
use tokio::{signal, task::AbortHandle};
use tower_sessions::cookie::Key;
// use tower_sessions_sqlx_store::SqliteStore;
use serde::Deserialize;
use std::fs::read_to_string;
use log::LevelFilter;
use sqlx::postgres::{PgConnectOptions};
// use tower_sessions_core::session_store::ExpiredDeletion;
use tower_sessions::{session_store::ExpiredDeletion, Expiry, Session};
use tower_sessions_surrealdb_store::{SurrealSessionStore};

use crate::{
    users::Backend,
    web::{auth, protected},
};

pub struct App {
    db: surrealdb::Surreal<C>,
}

impl App {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        /*
        println!("Creating connection string.");
        let options = SqliteConnectOptions::from_str("postgres://postgres:password1@localhost:5432?mode=rwc")?
        .journal_mode(SqliteJournalMode::Delete)
		.create_if_missing(true);

        // let pool = SqlitePoolOptions::new().connect_with(options).await?;
        // Ok(pool)
        println!("Connecting to database...");
        let db = SqlitePool::connect_with(options).await?;
        println!("Done.");
        // sqlx::migrate!().run(&db).await?;
        */
        // Modifying options parsed from a string
        let opts: PgConnectOptions = "postgres://postgr<Store: SessionStores:password1@localhost:5432".parse()?;

        // Change the log verbosity level for queries.
        // Information about SQL queries is logged at `DEBUG` level by default.
        // opts = opts.log_statements(log::LevelFilter::Trace);

        // let db = PgPool::connect_with(opts).await?;

        let db = surrealdb::Surreal::new::<surrealdb::engine::local::Mem>(())
            .await
            .expect("Surreal initialization failure");
        db.use_ns("testing")
            .await
            .expect("Surreal namespace initialization failure");
        db.use_db("testing")
            .await
            .expect("Surreal database initialization failure");

        Ok(Self { db })
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        // Session layer.
        //
        // This uses `tower-sessions` to establish a layer that will provide the session
        // as a request extension.
        let session_store = SurrealSessionStore::new(db.clone(), "sessions".to_string());
        session_store.migrate().await?;

        let deletion_task = tokio::task::spawn(
            session_store
                .clone()
                //.delete_expired(),
                .continuously_delete_expired(tokio::time::Duration::from_secs(60)),
        );

        // Generate a cryptographic key to sign the session cookie.
        let key = Key::generate();

        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_expiry(Expiry::OnInactivity(Duration::days(1)))
            .with_signed(key);

        // Auth service.
        //
        // This combines the session layer with our backend to establish the auth
        // service which will provide the auth session as a request extension.
        let backend = Backend::new(self.db);
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        let app = protected::router()
            .route_layer(login_required!(Backend, login_url = "/login"))
            .merge(auth::router())
            .layer(MessagesManagerLayer)
            .layer(auth_layer);

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

        // Ensure we use a shutdown signal to abort the deletion task.
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(shutdown_signal(deletion_task.abort_handle()))
            .await?;

        deletion_task.await??;

        Ok(())
    }
}

async fn shutdown_signal(deletion_task_abort_handle: AbortHandle) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { deletion_task_abort_handle.abort() },
        _ = terminate => { deletion_task_abort_handle.abort() },
    }
}
