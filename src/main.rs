use std::sync::LazyLock;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
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

static DB: LazyLock<Surreal<Client>> = LazyLock::new(Surreal::init);

mod error {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::response::Response;
    use axum::Json;
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum Error {
        #[error("database error")]
        Db,
    }

    impl IntoResponse for Error {
        fn into_response(self) -> Response {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(self.to_string())).into_response()
        }
    }

    impl From<surrealdb::Error> for Error {
        fn from(error: surrealdb::Error) -> Self {
            eprintln!("{error}");
            Self::Db
        }
    }
}

mod routes {
    use crate::error::Error;
    use crate::DB;

    use axum::{extract::Path, Json};
    use faker_rand::en_us::names::FirstName;
    use surrealdb::{RecordId, opt::auth::Record};
    use serde::{Deserialize, Serialize};

    const PERSON: &str = "person";

    #[derive(Serialize, Deserialize, Clone)]
    pub struct PersonData {
        name: String,
    }

    #[derive(Serialize, Deserialize)]
    pub struct Person {
        name: String,
        id: RecordId,
    }

    pub async fn paths() -> &'static str {
        r#"
-----------------------------------------------------------------------------------------------------------------------------------------
        PATH                |           SAMPLE COMMAND                                                                                  
-----------------------------------------------------------------------------------------------------------------------------------------
/session: See session data  |  curl -X GET    -H "Content-Type: application/json"                      http://localhost:8080/session
                            |
/person/{id}:               |
  Create a person           |  curl -X POST   -H "Content-Type: application/json" -d '{"name":"John"}' http://localhost:8080/person/one
  Update a person           |  curl -X PUT    -H "Content-Type: application/json" -d '{"name":"Jane"}' http://localhost:8080/person/one
  Get a person              |  curl -X GET    -H "Content-Type: application/json"                      http://localhost:8080/person/one
  Delete a person           |  curl -X DELETE -H "Content-Type: application/json"                      http://localhost:8080/person/one
                            |
/people: List all people    |  curl -X GET    -H "Content-Type: application/json"                      http://localhost:8080/people

/new_user:  Create a new record user
/new_token: Get instructions for a new token if yours has expired"#
    }

    pub async fn session() -> Result<Json<String>, Error> {
        let res: Option<String> = DB.query("RETURN <string>$session").await?.take(0)?;

        Ok(Json(res.unwrap_or("No session data found!".into())))
    }

    pub async fn create_person(
        id: Path<String>,
        Json(person): Json<PersonData>,
    ) -> Result<Json<Option<Person>>, Error> {
        let person = DB.create((PERSON, &*id)).content(person).await?;
        Ok(Json(person))
    }

    pub async fn read_person(id: Path<String>) -> Result<Json<Option<Person>>, Error> {
        let person = DB.select((PERSON, &*id)).await?;
        Ok(Json(person))
    }

    pub async fn update_person(
        id: Path<String>,
        Json(person): Json<PersonData>,
    ) -> Result<Json<Option<Person>>, Error> {
        let person = DB.update((PERSON, &*id)).content(person).await?;
        Ok(Json(person))
    }

    pub async fn delete_person(id: Path<String>) -> Result<Json<Option<Person>>, Error> {
        let person = DB.delete((PERSON, &*id)).await?;
        Ok(Json(person))
    }

    pub async fn list_people() -> Result<Json<Vec<Person>>, Error> {
        let people = DB.select(PERSON).await?;
        Ok(Json(people))
    }

    #[derive(Serialize, Deserialize)]
    struct Params<'a> {
        name: &'a str,
        pass: &'a str,
    }

    pub async fn make_new_user() -> Result<String, Error> {
        let name = rand::random::<FirstName>().to_string();
        let pass = rand::random::<FirstName>().to_string();
        let jwt = DB
            .signup(Record {
                access: "account",
                namespace: "test",
                database: "test",
                params: Params {
                    name: &name,
                    pass: &pass,
                },
            })
            .await?
            .into_insecure_token();
        Ok(format!("New user created!\n\nName: {name}\nPassword: {pass}\nToken: {jwt}\n\nTo log in, use this command:\n\nsurreal sql --ns test --db test --pretty --token \"{jwt}\""))
    }

    pub async fn get_new_token() -> String {
        let command = r#"curl -X POST -H "Accept: application/json" -d '{"ns":"test","db":"test","ac":"account","user":"your_username","pass":"your_password"}' http://localhost:8000/signin"#;
        format!("Need a new token? Use this command:\n\n{command}\n\nThen log in with surreal sql --ns test --db test --pretty --token YOUR_TOKEN_HERE")
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
    type Error = std::convert::Infallible;

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
        // TODO: Implement getting user by connectin to the surrealdb
        Ok(None)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session_store = SurrealSessionStore::new(DB.clone(), "session".to_string());
    let expired_session_cleanup_interval: u64 = 1;
    tokio::task::spawn(session_store.clone().continuously_delete_expired(
        tokio::time::Duration::from_secs(60 * expired_session_cleanup_interval),
    ));

    let session_layer = SessionManagerLayer::new(session_store)
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
        .with_graceful_shutdown(shutdown_signal(deletion_task.abort_handle()))
        .await?;

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
    Ok(())
}
