pub mod platform;
pub mod model;
pub mod proxy;
pub mod route;
pub mod stats;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        .service(
            web::scope("/api")
                // 平台
                .route("/platforms", web::get().to(platform::list))
                .route("/platforms", web::post().to(platform::create))
                .route("/platforms/presets", web::get().to(platform::list_presets))
                .route("/platforms/{id}", web::get().to(platform::get))
                .route("/platforms/{id}", web::put().to(platform::update))
                .route("/platforms/{id}", web::delete().to(platform::delete))
                // 模型
                .route("/models", web::get().to(model::list))
                .route("/models", web::post().to(model::create))
                .route("/models/{id}", web::get().to(model::get))
                .route("/models/{id}", web::put().to(model::update))
                .route("/models/{id}", web::delete().to(model::delete))
                // 代理
                .route("/proxies", web::get().to(proxy::list))
                .route("/proxies", web::post().to(proxy::create))
                .route("/proxies/{id}", web::get().to(proxy::get))
                .route("/proxies/{id}", web::put().to(proxy::update))
                .route("/proxies/{id}", web::delete().to(proxy::delete))
                .route("/proxies/{id}/start", web::post().to(proxy::start))
                .route("/proxies/{id}/stop", web::post().to(proxy::stop))
                // 路由
                .route("/proxies/{proxy_id}/routes", web::get().to(route::list))
                .route("/proxies/{proxy_id}/routes", web::post().to(route::create))
                .route("/routes/{id}", web::put().to(route::update))
                .route("/routes/{id}", web::delete().to(route::delete))
                .route("/routes/{route_id}/backends", web::get().to(route::list_backends))
                .route("/routes/{route_id}/backends", web::post().to(route::add_backend))
                .route("/backends/{id}", web::put().to(route::update_backend))
                .route("/backends/{id}", web::delete().to(route::delete_backend))
                // 统计
                .route("/stats/overview", web::get().to(stats::overview))
        );
}
