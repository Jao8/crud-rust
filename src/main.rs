use postgres::Error as PostgresError;
use postgres::{Client, NoTls};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

#[macro_use]
extern crate serde_derive;

// Model for User(id, name, email)
#[derive(Serialize, Deserialize)]
struct User {
    id: Option<i32>,
    name: String,
    email: String,
    password: String,
}

//DB Connection
const DB_URL: &str = env!("DATABASE_URL");

// Constants Headers
const OK_RESPONSE: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n";
const NOT_FOUND: &str = "HTTP/1.1 404 NOT FOUND\r\n";
const INTERNAL_SERVER_ERROR: &str = "HTTP/1.1 500 INTERNAL SERVER ERROR\r\n";

//main function
fn main() {
    // Set Database
    if let Err(e) = set_databse() {
        println!("Error setting database: {}", e);
        return;
    }

    // Start the server
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();
    println!("Server started at port 8080");

    // handle incoming requests
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                handle_client(&mut stream);
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}

fn handle_client(mut stream: &TcpStream) {
    // Read the request
    let mut buffer = [0; 1024];
    let mut request = String::new();

    match stream.read(&mut buffer) {
        Ok(size) => {
            request.push_str(String::from_utf8_lossy(&buffer[..size]).as_ref());

            // Handle the requests
            let (status_line, content) = match &*request {
                r if r.starts_with("GET /users") => handle_get_all_users_request(),
                r if r.starts_with("GET /users/") => handle_get_request(r),
                r if r.starts_with("POST /users") => handle_post_request(r),
                r if r.starts_with("PUT /users/") => handle_put_request(r),
                r if r.starts_with("DELETE /users/") => handle_delete_request(r),
                _ => (NOT_FOUND, "Not Found".to_string()),
            };

            // Send the response
            let response = format!("{}{}", status_line, content);
            stream.write(response.as_bytes()).unwrap();
        }
        Err(e) => {
            println!("Error reading request: {}", e);
            return;
        }
    }
}

fn set_databse() -> Result<(), PostgresError> {
    // Connect to the database
    let mut client = Client::connect(DB_URL, NoTls)?;

    // Create the users table
    client.execute(
        "
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            password TEXT NOT NULL
        )
    ",
        &[],
    )?;
    Ok(())
}

//get_id function
fn get_id(request: &str) -> &str {
    // Get the id from the request
    request // /users/1
        .split("/") // ["", "users", "1"]
        .nth(2) // "1"
        .unwrap_or_default() // "1"
        .split_whitespace() // "1"
        .next() // "1"
        .unwrap_or_default() // "1"
}

// deserialize the user from the request body without the id
fn get_user_request_body(request: &str) -> Result<User, serde_json::Error> {
    serde_json::from_str(request.split("\r\n\r\n").last().unwrap_or_default()) // {"name": "John", "email": "john@example", "password": "password"}
}

// Controllers

fn handle_get_all_users_request() -> (String, String) {
    match Client::connect(DB_URL, NoTls) {
        Ok(mut client) => {
            let mut users = vec![];
            for row in client.query("SELECT * FROM users", &[]).unwrap() {
                users.push(User {
                    id: row.get(0),
                    name: row.get(1),
                    email: row.get(2),
                    password: row.get(3),
                });
            }
            (
                OK_RESPONSE.to_string(),
                serde_json::to_string(&users).unwrap(),
            )
        }
        _ => (INTERNAL_SERVER_ERROR.to_string(), "Error".to_string()),
    }
}

fn handle_get_request(request: &str) -> (String, String) {
    match (
        get_id(&request).parse::<i32>().unwrap(),
        Client::connect(DB_URL, NoTls).map_err(PostgresError::from),
    ) {
        (id, Ok(mut client)) => {
            match client.query_one("SELECT * FROM users WHERE id = $1", &[&id]) {
                Ok(row) => {
                    let user = User {
                        id: row.get(0),
                        name: row.get(1),
                        email: row.get(2),
                        password: row.get(3),
                    };
                    (
                        OK_RESPONSE.to_string(),
                        serde_json::to_string(&user).unwrap(),
                    )
                }
                Err(_) => (NOT_FOUND.to_string(), "User not found".to_string()),
            }
        }
        _ => (INTERNAL_SERVER_ERROR.to_string(), "Error".to_string()),
    }
}

fn handle_post_request(request: &str) -> (String, String) {
    match (
        get_user_request_body(&request),
        Client::connect(DB_URL, NoTls),
    ) {
        (Ok(user), Ok(mut client)) => {
            // Insert the user
            client
                .execute(
                    "INSERT INTO users (name, email, password) VALUES ($1, $2, $3)",
                    &[&user.name, &user.email, &user.password],
                )
                .unwrap();

            // Return the response
            (
                OK_RESPONSE.to_string(),
                serde_json::to_string(&user).unwrap(),
            )
        }
        _ => (
            INTERNAL_SERVER_ERROR.to_string(),
            "Error creating user".to_string(),
        ),
    }
}
fn handle_put_request(request: &str) -> (String, String) {
    match (
        get_id(&request).parse::<i32>().unwrap(),
        get_user_request_body(&request),
        Client::connect(DB_URL, NoTls),
    ) {
        (id, Ok(user), Ok(mut client)) => {

            let mut password = user.password.clone();

            // Check if the user passwrod is a hash
            if password.len() < 20 {
                // Hash the password
                password = bcrypt::hash(&password, bcrypt::DEFAULT_COST).unwrap();
            }

            // Update the user
            client
                .execute(
                    "UPDATE users SET name = $1, email = $2, password = $3 WHERE id = $4",
                    &[&user.name, &user.email, &password, &id],
                )
                .unwrap();

            // Return the response
            (
                OK_RESPONSE.to_string(),
                "User updated".to_string(),
            )
        }
        _ => (
            INTERNAL_SERVER_ERROR.to_string(),
            "Error updating user".to_string(),
        ),
    }
}
fn handle_delete_request(request: &str) -> (String, String) {
    match (
        get_id(&request).parse::<i32>().unwrap(),
        Client::connect(DB_URL, NoTls),
    ) {
        (id, Ok(mut client)) => {
            // Delete the user
            let rows_affected = client
                .execute("DELETE FROM users WHERE id = $1", &[&id])
                .unwrap();

            if rows_affected == 0 {
                return (NOT_FOUND.to_string(), "User not found".to_string());
            }

            // Return the response
            (OK_RESPONSE.to_string(), "User deleted".to_string())
        }
        _ => (
            INTERNAL_SERVER_ERROR.to_string(),
            "Error deleting user".to_string(),
        ),
    }
}
