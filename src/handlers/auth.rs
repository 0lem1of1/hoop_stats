use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::IntoResponse,
    Json,
    response::Response,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use std::env;

use crate::models::{
    Claims, SigninRequest, SigninResponse, SignupRequest, SignupResponse,
    ForgotPasswordRequest, ResetPasswordRequest, WebhookPayload,
    SendGridPayload, Personalization, EmailAddress, Content,
};
use crate::AppState;

pub async fn signup(
    State(state): State<AppState>,
    Json(payload): Json<SignupRequest>,
) -> Result<(StatusCode, Json<SignupResponse>), StatusCode> {
    
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .to_string();

    
    let result = sqlx::query!(
        "INSERT INTO users (email, password_hash, favorite_player_id) VALUES ($1, $2, $3) RETURNING id",
        payload.email,
        password_hash,
        payload.favorite_player_id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e {
            if db_err.is_unique_violation() {
                return StatusCode::CONFLICT; 
            }
        }
        eprintln!("Failed to insert user: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::CREATED,
        Json(SignupResponse {
            message: "User created successfully".to_string(),
            user_id: result.id,
        }),
    ))
}

pub async fn signin(
    State(state): State<AppState>,
    Json(payload): Json<SigninRequest>,
) -> Result<Json<SigninResponse>, StatusCode> {

   
    let record = sqlx::query!(
        "SELECT id, password_hash FROM users WHERE email = $1",
        payload.email
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user = match record {
        Some(u) => u,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    
    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let is_valid = Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .is_ok();

    if !is_valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: user.id,
        exp: expiration,
        purpose: "password_check".to_string(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(SigninResponse { token }))
}

pub async fn auth_guard(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response<Body>, StatusCode> {
    
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !auth_header.starts_with("Bearer ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = &auth_header[7..];

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if token_data.claims.purpose != "password_check" {
        return Err(StatusCode::UNAUTHORIZED);
    }

    req.extensions_mut().insert(token_data.claims);

    Ok(next.run(req).await.into_response())
}

pub async fn forgot_password(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> Result<StatusCode, StatusCode> {
    let user = sqlx::query!(
        "SELECT id, email FROM users WHERE email = $1",
        payload.email
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user = match user {
        Some(u) => u,
        None => return Ok(StatusCode::OK), // <--- for security as ,  the not found could be used to enumerate emial addresses
    };

    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(1))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: user.id,
        exp: expiration,
        purpose: "password_reset".to_string(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let reset_link = format!("http://localhost:3000/reset-password?token={}", token);

    let api_key = env::var("SENDGRID_API_KEY")
        .map_err(|_| {
            eprintln!("SENDGRID_API_KEY not set");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let from_email = env::var("SENDGRID_FROM_EMAIL")
        .map_err(|_| {
            eprintln!("SENDGRID_FROM_EMAIL not set — must be a verified sender in SendGrid");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    eprintln!("[DEBUG] Sending password reset email to {} from {}", user.email, from_email);

    let email_payload = SendGridPayload {
        personalizations: vec![Personalization {
            to: vec![EmailAddress {
                email: user.email.clone(),
                name: None,
            }],
            subject: "Password Reset - Hoop Stats".to_string(),
        }],
        from: EmailAddress {
            email: from_email,
            name: Some("Hoop Stats".to_string()),
        },
        subject: "Password Reset - Hoop Stats".to_string(),
        content: vec![Content {
            content_type: "text/html".to_string(),
            value: format!(
                "<h2>Password Reset</h2><p>Click the link below to reset your password:</p><p><a href=\"{}\">Reset Password</a></p><p>This link expires in 1 hour.</p>",
                reset_link
            ),
        }],
    };

    let response = state.reqwest_client
        .post(&state.webhook_url)
        .bearer_auth(&api_key)
        .json(&email_payload)
        .send()
        .await
        .map_err(|e| {
            eprintln!("Failed to send email via SendGrid: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("SendGrid returned error {}: {}", status, body);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    eprintln!("Password reset email sent to {}", user.email);
    Ok(StatusCode::OK)
}

pub async fn reset_password(
    State(state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let token_data = decode::<Claims>(
        &payload.token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if token_data.claims.purpose != "password_reset" {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(payload.new_password.as_bytes(), &salt)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .to_string();

    sqlx::query!(
        "UPDATE users SET password_hash = $1 WHERE id = $2",
        password_hash,
        token_data.claims.sub
    )
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "message": "Password reset successful"
    })))
}

pub async fn webhook_receiver(
    State(state): State<AppState>,
    Json(payload): Json<WebhookPayload>,
) -> Result<StatusCode, StatusCode> {

    let api_key = env::var("SENDGRID_API_KEY")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let email_payload = SendGridPayload {
        personalizations: vec![Personalization {
            to: vec![EmailAddress {
                email: payload.email.clone(),
                name: None,
            }],
            subject: "Password Reset".to_string(),
        }],
        from: EmailAddress {
            email: "noreply@hoopstats.app".to_string(),
            name: Some("Hoop Stats Support".to_string()),
        },
        subject: "Password Reset".to_string(),
        content: vec![Content {
            content_type: "text/plain".to_string(),
            value: format!("Reset your password using this link: {}", payload.reset_link),
        }],
    };

    let result = state.reqwest_client
        .post(&state.webhook_url)
        .bearer_auth(api_key)
        .json(&email_payload)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !result.status().is_success() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(StatusCode::OK)
}