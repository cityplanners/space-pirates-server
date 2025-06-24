use std::sync::LazyLock;
use serde::{Deserialize};
use axum::{Router, routing::{delete, get, post, put}};
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};
use tokio::net::TcpListener;
use axum_login::{
    AuthnBackend,
    AuthUser,
    UserId,
    login_required,
    tower_sessions::{ExpiredDeletion, Expiry, SessionManagerLayer},
    AuthManagerLayerBuilder,
};
use axum_messages::MessagesManagerLayer;
use async_trait::async_trait;
use time::Duration;
use tokio::{signal, task::AbortHandle};
use tower::ServiceBuilder;
use tower_sessions_surrealdb_store::SurrealSessionStore;

pub static DB: LazyLock<Surreal<Client>> = LazyLock::new(Surreal::init);

use crate::web::error::Error;
use crate::web::routes;
use crate::web::protected;
use crate::web::auth;
use crate::users::Backend;

pub struct App {
}

impl App {

    pub async fn shutdown_signal(deletion_task_abort_handle: AbortHandle) {
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

    pub async fn serve() -> Result<(), Box<dyn std::error::Error>> {

        let db = surrealdb::Surreal::new::<Ws>("localhost:8000")
            .await
            .expect("Surreal initialization failure");
        db.use_ns("testing")
            .await
            .expect("Surreal namespace initialization failure");
        db.use_db("testing")
            .await
            .expect("Surreal database initialization failure");

        let session_store = SurrealSessionStore::new(db.clone(), "session".to_string());
        let expired_session_cleanup_interval: u64 = 1;
        let deletion_task = tokio::task::spawn(session_store.clone().continuously_delete_expired(
            tokio::time::Duration::from_secs(60 * expired_session_cleanup_interval),
        ));

        let session_layer = SessionManagerLayer::new(session_store.clone())
            .with_secure(false)
            .with_expiry(Expiry::OnInactivity(Duration::days(1)));

        let session_service = ServiceBuilder::new().layer(
            SessionManagerLayer::new(session_store)
                .with_secure(false)
                .with_expiry(Expiry::OnInactivity(Duration::days(1))),
        );

        // Auth service.
        //
        // This combines the session layer with our backend to establish the auth
        // service which will provide the auth session as a request extension.
        let backend = Backend::new();
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        let app = protected::router()
            .route_layer(login_required!(Backend, login_url = "/login"))
            .merge(auth::router())
            .layer(MessagesManagerLayer)
            .layer(auth_layer);

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

        // Ensure we use a shutdown signal to abort the deletion task.
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(Self::shutdown_signal(deletion_task.abort_handle()))
            .await?;

        /*
        DB.connect::<Ws>("localhost:8000").await?;

        DB.signin(Root {
            username: "root",
            password: "root",
        })
        .await?;

        DB.use_ns("test").use_db("test").await?;

        DB.query(
            "
        DEFINE TABLE IF NOT EXISTS person SCHEMALESS
            PERMISSIONS FOR 
                CREATE, SELECT WHERE $auth,
                FOR UPDATE, DELETE WHERE created_by = $auth;
        DEFINE FIELD IF NOT EXISTS name ON TABLE person TYPE string;
        DEFINE FIELD IF NOT EXISTS created_by ON TABLE person VALUE $auth READONLY;

        DEFINE INDEX IF NOT EXISTS unique_name ON TABLE user FIELDS name UNIQUE;
        DEFINE ACCESS IF NOT EXISTS account ON DATABASE TYPE RECORD
        SIGNUP ( CREATE user SET name = $name, pass = crypto::argon2::generate($pass) )
        SIGNIN ( SELECT * FROM user WHERE name = $name AND crypto::argon2::compare(pass, $pass) )
        DURATION FOR TOKEN 15m, FOR SESSION 12h
    ;",
        )
        .await?;

        // Session layer.
        //
        // This uses `tower-sessions` to establish a layer that will provide the session
        // as a request extension.
        // let db = surrealdb::Surreal::new::<surrealdb::engine::local::Mem>(())
            // .await
            // .expect("Surreal initialization failure");
        DB.use_ns("testing")
            .await
            .expect("Surreal namespace initialization failure");
        DB.use_db("testing")
            .await
            .expect("Surreal database initialization failure");

        let listener = TcpListener::bind("localhost:8080").await?;
        let router = Router::new()
            .route("/", get(routes::paths))
            .route("/person/{{:id}}", post(routes::create_person))
            .route("/person/{{:id}}", get(routes::read_person))
            .route("/person/{{:id}}", put(routes::update_person))
            .route("/person/{{:id}}", delete(routes::delete_person))
            .route("/people", get(routes::list_people))
            .route("/session", get(routes::session))
            .route("/new_user", get(routes::make_new_user))
            .route("/new_token", get(routes::get_new_token));
        axum::serve(listener, router).await?;
        */
        Ok(())
    }
}

/*
#[derive(Debug, Clone)]
struct User {
    id: i64,
    pw_hash: Vec<u8>,
}

impl AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        &self.pw_hash
    }
}

#[derive(Debug, Clone)]
pub struct Backend {
    // db: *LazyLock<Surreal<Client>>,
}

// This allows us to extract the authentication fields from forms. We use this
// to authenticate requests with the backend.
#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

impl Backend {
    pub fn new(/*db: Surreal<Client>*/) -> Self {
        Self { /* db */ }
    }
}
#[async_trait]
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = Error;

    async fn authenticate(
        &self,
        Credentials { username, password, next }: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        // TODO: Implement authentication using surrealdb
        Ok(None)
    }

    async fn get_user(
        &self,
        user_id: &UserId<Self>,
    ) -> Result<Option<Self::User>, Self::Error> {
        // TODO: Implement getting user by connection to the surrealdb
        Ok(None)
    }
}

*/
