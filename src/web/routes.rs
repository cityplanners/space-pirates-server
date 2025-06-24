use crate::web::error::Error;
use crate::web::app::DB;

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
