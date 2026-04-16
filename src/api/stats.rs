use actix_web::{web, HttpResponse};
use crate::db::DbPool;
use crate::error::AppResult;

pub async fn overview(db: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let db = db.into_inner();
    let stats = web::block(move || crate::db::stats::overview(&db))
        .await.map_err(|e| crate::error::AppError::Internal(e.to_string()))??;
    Ok(HttpResponse::Ok().json(stats))
}
