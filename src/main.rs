use std::io;
use actix_web::{web::{self}, App, HttpResponse, HttpServer, Responder};
use mongodb::{Collection, Database, options::{ClientOptions, FindOneOptions, UpdateOptions}, Client};
use bson::{doc, oid::ObjectId};
use serde::{Deserialize, Serialize};
use futures_util::stream::StreamExt;
use std::env;
use dotenv::dotenv;

// Define a struct to represent a user
#[derive(Debug, Serialize, Deserialize, Clone)]
struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

// Define an Actix web route to create a new user
async fn create_user(db: web::Data<Database>, user: web::Json<User>) -> impl Responder {
    // Get a handle to the "users" collection
    let collection: Collection<User> = db.collection("users");

    let user_id = ObjectId::new();
    let new_user = User {
        id: Some(user_id),
        name: user.name.clone(),
        email: user.email.clone(),
    };
    // Insert the new user into the collection
    let result = collection.insert_one(&new_user, None).await;
    // Return the new user ID
    match result {
        Ok(_) => HttpResponse::Ok().json(new_user),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

async fn update_user(db: web::Data<Database>, info: web::Path<String>, user: web::Json<User>) -> impl Responder {
    // Parse the user ID from the request path
    let user_id = match ObjectId::parse_str(&info.to_string()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    // Get a handle to the "users" collection
    let collection: Collection<User> = db.collection("users");

    // Find the user with the given ID
    let filter = doc! { "_id": user_id };
    let options = FindOneOptions::builder().build();
    let existing_user = match collection.find_one(filter, options).await {
        Ok(Some(user)) => user,
        Ok(None) => return HttpResponse::NotFound().finish(),
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    // Create an updated user with the new data
    let updated_user = User {
        id: Some(user_id),
        name: user.name.clone().or(existing_user.name),
        email: user.email.clone().or(existing_user.email),
    };

    // Update the user
    let result = update_user_in_db(&collection, &user_id, &updated_user).await;
    if result {
        HttpResponse::Ok().json(updated_user)
    } else {
        HttpResponse::InternalServerError().finish()
    }
}

async fn update_user_in_db(collection: &Collection<User>, user_id: &ObjectId, updated_user: &User) -> bool {
    let filter = doc! {"_id": user_id};
    let options = UpdateOptions::builder().upsert(false).build();
    let update_doc = doc! {
        "$set": {
            "name": updated_user.name.clone(),
            "email": updated_user.email.clone()
        }
    };
    match collection.update_one(filter, update_doc, options).await {
        Ok(result) => result.modified_count > 0,
        Err(e) => {
            println!("Error updating user: {}", e);
            false
        }
    }
}

async fn delete_user(db: web::Data<Database>, info: web::Path<String>) -> impl Responder {
    // Parse the user ID from the request path
    let user_id = match ObjectId::parse_str(&info.to_string()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    // Get a handle to the "users" collection
    let collection: Collection<User> = db.collection("users");

    // Find the user with the given ID
    let filter = doc! { "_id": user_id };
    let user = match collection.find_one(filter, None).await {
        Ok(Some(user)) => user,
        Ok(None) => return HttpResponse::NotFound().finish(),
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    // Delete the user with the given ID
    let filter = doc! { "_id": user_id };
    let result = collection.delete_one(filter, None).await;

    // Return the deleted user in the response
    match result {
        Ok(delete_result) => {
            if delete_result.deleted_count == 1 {
                HttpResponse::Ok().json(user)
            } else {
                HttpResponse::NotFound().finish()
            }
        },
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

// Define an Actix web route to get a user by ID
async fn get_user(db: web::Data<Database>, info: web::Path<String>) -> impl Responder {
    // Parse the user ID from the request path
    let user_id = match ObjectId::parse_str(&info.to_string()) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    // Get a handle to the "users" collection
    let collection: Collection<User> = db.collection("users");

    // Find the user with the given ID
    let filter = doc! { "_id": user_id };
    let result = collection.find_one(filter, None).await;
    // Return the user if found, or a 404 if not found
    match result {
        Ok(Some(user)) => HttpResponse::Ok().json(user),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

async fn get_all_user(db: web::Data<Database>, _: web::Path<()>) -> impl Responder {
    let collection: Collection<User> = db.collection("users");
    let mut cursor = collection.find(doc! {}, None).await.expect("Failed to execute find query");
    let mut users = vec![];
    // Iterate over the cursor and push each user to the vector
    while let Some(user) = cursor.next().await {
        match user {
            Ok(user) => users.push(user),
            Err(_) => return HttpResponse::InternalServerError().finish(),
        }
    }
    // If the vector is empty, return 404 Not Found
    if users.is_empty() {
        return HttpResponse::NotFound().finish();
    }
    // Return the vector of users as JSON
    HttpResponse::Ok().json(users)
}

async fn start_server() -> io::Result<()> {
    let db_host = env::var("MONGO_DB").unwrap();
    // Configure the MongoDB client
    let client_options = ClientOptions::parse(db_host).await.unwrap();
    let client = Client::with_options(client_options).unwrap();
    let db = client.database("test");
    // Run the Actix web server
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db.clone()))
            .service(web::resource("/users")
                .route(web::post().to(create_user))
                .route(web::get().to(get_all_user)))
            .service(web::resource("/users/{id}")
                .route(web::get().to(get_user))
                .route(web::put().to(update_user))
                .route(web::delete().to(delete_user))
        )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

fn main() -> io::Result<()> {
    dotenv().ok();
    // Start the Actix web server inside an async block
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        start_server().await.unwrap();
    });
    Ok(())
}