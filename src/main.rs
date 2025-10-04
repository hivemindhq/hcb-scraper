use actix_web::{web, App, HttpServer, Responder};
use serde::Serialize;
use reqwest::Client;
use scraper::{Html, Selector};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Instant, Duration};
use once_cell::sync::Lazy;
use chrono::Utc;

#[derive(Serialize, Clone)]
struct DonationData {
    raised_text: String,
    goal_text: String,
    raised: f64,
    goal: f64,
    progress_percent: f64,
    fetched_at: String,
    source_url: String,
}

static CACHE: Lazy<Mutex<HashMap<String, (Instant, DonationData)>>> = Lazy::new(|| Mutex::new(HashMap::new()));
const CACHE_TTL: Duration = Duration::from_secs(60);

fn parse_money(s: &str) -> f64 {
    s.replace(",", "").replace("$", "").parse::<f64>().unwrap_or(0.0)
}

async fn fetch_data(org_id: &str) -> DonationData {
    let url = format!("https://hcb.hackclub.com/donations/start/{}", org_id);
    let client = Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", "hcb-scraper/1.0")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let document = Html::parse_document(&resp);

    let money_re = Regex::new(r"\$\s?[\d,]+(?:\.\d+)?").unwrap();
    let matches: Vec<&str> = money_re.find_iter(&resp).map(|m| m.as_str()).collect();
    let raised_text = matches.get(0).unwrap_or(&"$0").to_string();
    let goal_text = matches.get(1).unwrap_or(&"$0").to_string();
    let raised = parse_money(&raised_text);
    let goal = parse_money(&goal_text);
    let progress = if goal > 0.0 { (raised / goal) * 100.0 } else { 0.0 };

    DonationData {
        raised_text,
        goal_text,
        raised,
        goal,
        progress_percent: progress,
        fetched_at: Utc::now().to_rfc3339(),
        source_url: url,
    }
}

async fn handler(path: web::Path<String>) -> impl Responder {
    let org_id = path.into_inner();
    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let data = if let Some((time, cached_data)) = cache.get(&org_id) {
        if time.elapsed() < CACHE_TTL {
            cached_data.clone()
        } else {
            let new_data = fetch_data(&org_id).await;
            cache.insert(org_id.clone(), (Instant::now(), new_data.clone()));
            new_data
        }
    } else {
        let new_data = fetch_data(&org_id).await;
        cache.insert(org_id.clone(), (Instant::now(), new_data.clone()));
        new_data
    };

    web::Json(data)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/donations/{org_id}", web::get().to(handler))
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}
