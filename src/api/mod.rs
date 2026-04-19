pub mod platform;
pub mod model;
pub mod proxy;
pub mod route;
pub mod stats;
pub mod settings;
pub mod api_key;

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
                .route("/platforms/{id}/remote-models", web::get().to(platform::fetch_remote_models))
                // 模型
                .route("/models", web::get().to(model::list))
                .route("/models", web::post().to(model::create))
                .route("/models/{id}", web::get().to(model::get))
                .route("/models/{id}", web::put().to(model::update))
                .route("/models/{id}", web::delete().to(model::delete))
                .route("/models/{id}/test", web::post().to(model::test_connection))
                // 代理
                .route("/proxies", web::get().to(proxy::list))
                .route("/proxies", web::post().to(proxy::create))
                .route("/proxies/{id}", web::get().to(proxy::get))
                .route("/proxies/{id}", web::put().to(proxy::update))
                .route("/proxies/{id}", web::delete().to(proxy::delete))
                // 路由
                .route("/proxies/{proxy_id}/routes", web::get().to(route::list))
                .route("/proxies/{proxy_id}/routes", web::post().to(route::create))
                .route("/routes/{id}", web::put().to(route::update))
                .route("/routes/{id}", web::delete().to(route::delete))
                .route("/routes/{route_id}/backends", web::get().to(route::list_backends))
                .route("/routes/{route_id}/backends", web::post().to(route::add_backend))
                .route("/backends/{id}", web::put().to(route::update_backend))
                .route("/backends/{id}", web::delete().to(route::delete_backend))
                // API Keys
                .route("/api-keys", web::get().to(api_key::list))
                .route("/api-keys", web::post().to(api_key::create))
                .route("/api-keys/{id}", web::delete().to(api_key::delete))
                // 统计
                .route("/stats/overview", web::get().to(stats::overview))
                .route("/stats/proxy/{proxy_id}", web::get().to(stats::proxy_stats))
                .route("/stats/platform/{platform_id}", web::get().to(stats::platform_stats))
                // 设置
                .route("/settings", web::get().to(settings::get_config))
                .route("/settings", web::put().to(settings::update_config))
        );
}
