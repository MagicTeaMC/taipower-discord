use reqwest;
use dotenv::dotenv;
use serde::Deserialize;
use serde_json;
use chrono;
use serenity::{
    async_trait,
    model::{gateway::Ready, id::ChannelId},
    prelude::*,
};
use std::collections::HashMap;
use std::env;
use tokio::time::{interval, Duration};

#[derive(Debug, Deserialize, Clone)]
struct PowerData {
    #[serde(rename = "DateTime")]
    date_time: String,
    #[serde(rename = "aaData")]
    aa_data: Vec<PowerUnit>,
}

// Alternative structure for different API endpoints
#[derive(Debug, Deserialize, Clone)]
struct AlternativePowerData {
    #[serde(rename = "datas")]
    datas: Vec<PowerUnit>,
}

#[derive(Debug, Deserialize, Clone)]
struct PowerUnit {
    #[serde(rename = "機組類型")]
    unit_type: String,
    #[serde(rename = "機組名稱")]
    unit_name: String,
    #[serde(rename = "裝置容量(MW)")]
    capacity: String,
    #[serde(rename = "淨發電量(MW)")]
    generation: String,
    #[serde(rename = "淨發電量/裝置容量比(%)")]
    ratio: String,
    #[serde(rename = "備註")]
    remark: String,
}

// New structures for load data API
#[derive(Debug, Deserialize, Clone)]
struct LoadDataResponse {
    success: String,
    result: LoadResult,
    records: Vec<LoadRecord>,
}

#[derive(Debug, Deserialize, Clone)]
struct LoadResult {
    resource_id: String,
}

#[derive(Debug, Deserialize, Clone)]
struct LoadRecord {
    #[serde(rename = "curr_load")]
    current_load: Option<String>,
    #[serde(rename = "curr_util_rate")]
    current_util_rate: Option<String>,
    #[serde(rename = "fore_maxi_sply_capacity")]
    forecast_max_supply_capacity: Option<String>,
    #[serde(rename = "fore_peak_dema_load")]
    forecast_peak_demand_load: Option<String>,
    #[serde(rename = "fore_peak_resv_capacity")]
    forecast_peak_reserve_capacity: Option<String>,
    #[serde(rename = "fore_peak_resv_rate")]
    forecast_peak_reserve_rate: Option<String>,
    #[serde(rename = "fore_peak_resv_indicator")]
    forecast_peak_reserve_indicator: Option<String>,
    #[serde(rename = "fore_peak_hour_range")]
    forecast_peak_hour_range: Option<String>,
    #[serde(rename = "publish_time")]
    publish_time: Option<String>,
    #[serde(rename = "yday_date")]
    yesterday_date: Option<String>,
    #[serde(rename = "yday_maxi_sply_capacity")]
    yesterday_max_supply_capacity: Option<String>,
    #[serde(rename = "yday_peak_dema_load")]
    yesterday_peak_demand_load: Option<String>,
    #[serde(rename = "yday_peak_resv_capacity")]
    yesterday_peak_reserve_capacity: Option<String>,
    #[serde(rename = "yday_peak_resv_rate")]
    yesterday_peak_reserve_rate: Option<String>,
    #[serde(rename = "yday_peak_resv_indicator")]
    yesterday_peak_reserve_indicator: Option<String>,
    #[serde(rename = "real_hr_maxi_sply_capacity")]
    real_hour_max_supply_capacity: Option<String>,
    #[serde(rename = "real_hr_peak_time")]
    real_hour_peak_time: Option<String>,
}

#[derive(Debug)]
struct LoadData {
    current_load: f64,
    current_util_rate: f64,
    forecast_max_supply_capacity: f64,
    forecast_peak_demand_load: f64,
    forecast_peak_reserve_capacity: f64,
    forecast_peak_reserve_rate: f64,
    forecast_peak_reserve_indicator: String,
    forecast_peak_hour_range: String,
    publish_time: String,
    yesterday_max_supply_capacity: f64,
    yesterday_peak_demand_load: f64,
    yesterday_peak_reserve_capacity: f64,
    yesterday_peak_reserve_rate: f64,
    yesterday_peak_reserve_indicator: String,
    real_hour_max_supply_capacity: f64,
    real_hour_peak_time: String,
}

#[derive(Debug)]
struct PowerAnalysis {
    update_time: String,
    total_generation: f64,
    estimated_max_generation: f64,
    generation_by_type: HashMap<String, f64>,
    top_plant: (String, f64),
    top_unit: (String, f64),
    environmental_restrictions: i32,
    maintenance_count: i32,
    fault_count: i32,
    renewable_ratio: f64,
    private_ratio: f64,
}

#[derive(Debug)]
struct CombinedPowerData {
    power_analysis: PowerAnalysis,
    load_data: Option<LoadData>,
}

struct Handler {
    channel_id: ChannelId,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        
        let ctx = ctx.clone();
        let channel_id = self.channel_id;
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(600)); // 10 minutes
            
            loop {
                interval.tick().await;
                
                // Fetch both power generation and load data
                let power_analysis = match fetch_and_analyze_power_data().await {
                    Ok(analysis) => analysis,
                    Err(e) => {
                        println!("Error fetching power data: {:?}", e);
                        let error_msg = format!("❌ 無法取得台電發電資料: {}", e);
                        if let Err(why) = channel_id.say(&ctx.http, &error_msg).await {
                            println!("Error sending error message: {:?}", why);
                        }
                        continue;
                    }
                };
                
                let load_data = match fetch_load_data().await {
                    Ok(data) => Some(data),
                    Err(e) => {
                        println!("Error fetching load data: {:?}", e);
                        None
                    }
                };
                
                let combined_data = CombinedPowerData {
                    power_analysis,
                    load_data,
                };
                
                let message = format_combined_power_message(&combined_data);
                if let Err(why) = channel_id.say(&ctx.http, &message).await {
                    println!("Error sending message: {:?}", why);
                }
            }
        });
    }
}

async fn fetch_load_data() -> Result<LoadData, Box<dyn std::error::Error + Send + Sync>> {
    let url = "https://service.taipower.com.tw/data/opendata/apply/file/d006020/001.json";
    
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    
    println!("Fetching load data from: {}", url);
    
    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()).into());
    }
    
    let text = response.text().await?;
    println!("Load data response length: {} characters", text.len());
    
    let load_response: LoadDataResponse = serde_json::from_str(&text)?;
    
    // Process records to extract load data
    let mut current_load = 0.0;
    let mut current_util_rate = 0.0;
    let mut forecast_max_supply_capacity = 0.0;
    let mut forecast_peak_demand_load = 0.0;
    let mut forecast_peak_reserve_capacity = 0.0;
    let mut forecast_peak_reserve_rate = 0.0;
    let mut forecast_peak_reserve_indicator = "".to_string();
    let mut forecast_peak_hour_range = "".to_string();
    let mut publish_time = "".to_string();
    let mut yesterday_max_supply_capacity = 0.0;
    let mut yesterday_peak_demand_load = 0.0;
    let mut yesterday_peak_reserve_capacity = 0.0;
    let mut yesterday_peak_reserve_rate = 0.0;
    let mut yesterday_peak_reserve_indicator = "".to_string();
    let mut real_hour_max_supply_capacity = 0.0;
    let mut real_hour_peak_time = "".to_string();
    
    for record in load_response.records {
        if let Some(load) = record.current_load {
            current_load = load.parse().unwrap_or(0.0);
        }
        if let Some(rate) = record.current_util_rate {
            current_util_rate = rate.parse().unwrap_or(0.0);
        }
        if let Some(capacity) = record.forecast_max_supply_capacity {
            forecast_max_supply_capacity = capacity.parse().unwrap_or(0.0);
        }
        if let Some(demand) = record.forecast_peak_demand_load {
            forecast_peak_demand_load = demand.parse().unwrap_or(0.0);
        }
        if let Some(reserve) = record.forecast_peak_reserve_capacity {
            forecast_peak_reserve_capacity = reserve.parse().unwrap_or(0.0);
        }
        if let Some(rate) = record.forecast_peak_reserve_rate {
            forecast_peak_reserve_rate = rate.parse().unwrap_or(0.0);
        }
        if let Some(indicator) = record.forecast_peak_reserve_indicator {
            forecast_peak_reserve_indicator = indicator;
        }
        if let Some(hour_range) = record.forecast_peak_hour_range {
            forecast_peak_hour_range = hour_range;
        }
        if let Some(time) = record.publish_time {
            publish_time = time;
        }
        if let Some(capacity) = record.yesterday_max_supply_capacity {
            yesterday_max_supply_capacity = capacity.parse().unwrap_or(0.0);
        }
        if let Some(demand) = record.yesterday_peak_demand_load {
            yesterday_peak_demand_load = demand.parse().unwrap_or(0.0);
        }
        if let Some(reserve) = record.yesterday_peak_reserve_capacity {
            yesterday_peak_reserve_capacity = reserve.parse().unwrap_or(0.0);
        }
        if let Some(rate) = record.yesterday_peak_reserve_rate {
            yesterday_peak_reserve_rate = rate.parse().unwrap_or(0.0);
        }
        if let Some(indicator) = record.yesterday_peak_reserve_indicator {
            yesterday_peak_reserve_indicator = indicator;
        }
        if let Some(capacity) = record.real_hour_max_supply_capacity {
            real_hour_max_supply_capacity = capacity.parse().unwrap_or(0.0);
        }
        if let Some(time) = record.real_hour_peak_time {
            real_hour_peak_time = time;
        }
    }
    
    Ok(LoadData {
        current_load,
        current_util_rate,
        forecast_max_supply_capacity,
        forecast_peak_demand_load,
        forecast_peak_reserve_capacity,
        forecast_peak_reserve_rate,
        forecast_peak_reserve_indicator,
        forecast_peak_hour_range,
        publish_time,
        yesterday_max_supply_capacity,
        yesterday_peak_demand_load,
        yesterday_peak_reserve_capacity,
        yesterday_peak_reserve_rate,
        yesterday_peak_reserve_indicator,
        real_hour_max_supply_capacity,
        real_hour_peak_time,
    })
}

async fn fetch_and_analyze_power_data() -> Result<PowerAnalysis, Box<dyn std::error::Error + Send + Sync>> {
    // Try multiple endpoints
    let urls = vec![
        "https://www.taipower.com.tw/d006/loadGraph/loadGraph/data/genloadareaperc.json",
        "https://service.taipower.com.tw/data/opendata/apply/file/d006001/001.json",
        "https://www.taipower.com.tw/d006/loadGraph/loadGraph/data/genary.json"
    ];
    
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    
    for (i, url) in urls.iter().enumerate() {
        println!("Trying URL {}: {}", i + 1, url);
        
        match client.get(*url).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    println!("HTTP error for URL {}: {}", i + 1, response.status());
                    continue;
                }
                
                match response.text().await {
                    Ok(text) => {
                        println!("Response length: {} characters", text.len());
                        println!("First 200 chars: {}", &text[..std::cmp::min(200, text.len())]);
                        
                        // Try parsing as original format
                        if let Ok(power_data) = serde_json::from_str::<PowerData>(&text) {
                            return analyze_power_data_from_standard(power_data);
                        }
                        
                        // Try parsing as alternative format
                        if let Ok(alt_data) = serde_json::from_str::<AlternativePowerData>(&text) {
                            return analyze_power_data_from_alternative(alt_data);
                        }
                        
                        // If both fail, try extracting just the data array
                        if let Ok(units) = serde_json::from_str::<Vec<PowerUnit>>(&text) {
                            let power_data = PowerData {
                                date_time: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                                aa_data: units,
                            };
                            return analyze_power_data_from_standard(power_data);
                        }
                        
                        println!("Failed to parse JSON from URL {}", i + 1);
                    }
                    Err(e) => {
                        println!("Failed to get text from URL {}: {}", i + 1, e);
                        continue;
                    }
                }
            }
            Err(e) => {
                println!("Failed to fetch URL {}: {}", i + 1, e);
                continue;
            }
        }
    }
    
    Err("All API endpoints failed".into())
}

fn analyze_power_data_from_standard(data: PowerData) -> Result<PowerAnalysis, Box<dyn std::error::Error + Send + Sync>> {
    analyze_power_data(data.aa_data, data.date_time)
}

fn analyze_power_data_from_alternative(data: AlternativePowerData) -> Result<PowerAnalysis, Box<dyn std::error::Error + Send + Sync>> {
    let date_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    analyze_power_data(data.datas, date_time)
}

fn analyze_power_data(units: Vec<PowerUnit>, date_time: String) -> Result<PowerAnalysis, Box<dyn std::error::Error + Send + Sync>> {
    let mut total_generation = 0.0;
    let mut estimated_max_generation = 0.0;
    let mut generation_by_type: HashMap<String, f64> = HashMap::new();
    let mut plant_generation: HashMap<String, f64> = HashMap::new();
    let mut unit_generation: HashMap<String, f64> = HashMap::new();
    let mut environmental_restrictions = 0;
    let mut maintenance_count = 0;
    let mut fault_count = 0;
    let mut renewable_generation = 0.0;
    let mut private_generation = 0.0;
    
    for unit in &units {
        // Skip summary rows
        if unit.unit_name == "小計" {
            continue;
        }
        
        // Parse capacity and generation
        let capacity = parse_mw_value(&unit.capacity);
        let generation = parse_mw_value(&unit.generation);
        
        // Add to total generation
        total_generation += generation;
        estimated_max_generation += capacity;
        
        // Group by energy type
        let energy_type = clean_energy_type(&unit.unit_type);
        *generation_by_type.entry(energy_type.clone()).or_insert(0.0) += generation;
        
        // Track renewable energy (風力, 太陽能, 水力, 其它再生能源)
        if is_renewable(&energy_type) {
            renewable_generation += generation;
        }
        
        // Track private generation (民營電廠)
        if unit.unit_type.contains("民營電廠") {
            private_generation += generation;
        }
        
        // Extract plant name for top plant calculation
        if let Some(plant_name) = extract_plant_name(&unit.unit_name) {
            *plant_generation.entry(plant_name).or_insert(0.0) += generation;
        }
        
        // Track individual units
        if generation > 0.0 && !unit.unit_name.contains("小計") {
            unit_generation.insert(unit.unit_name.clone(), generation);
        }
        
        // Count issues based on remarks
        match unit.remark.as_str() {
            r if r.contains("環保限制") || r.contains("運轉限制") => environmental_restrictions += 1,
            r if r.contains("歲修") || r.contains("檢修") => maintenance_count += 1,
            r if r.contains("故障") => fault_count += 1,
            _ => {}
        }
    }
    
    // Find top plant and unit
    let top_plant = plant_generation
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(("未知".to_string(), 0.0));
    
    let top_unit = unit_generation
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(("未知".to_string(), 0.0));
    
    // Calculate ratios
    let renewable_ratio = if total_generation > 0.0 {
        (renewable_generation / total_generation) * 100.0
    } else {
        0.0
    };
    
    let private_ratio = if total_generation > 0.0 {
        (private_generation / total_generation) * 100.0
    } else {
        0.0
    };
    
    Ok(PowerAnalysis {
        update_time: date_time,
        total_generation,
        estimated_max_generation,
        generation_by_type,
        top_plant,
        top_unit,
        environmental_restrictions,
        maintenance_count,
        fault_count,
        renewable_ratio,
        private_ratio,
    })
}

fn parse_mw_value(value: &str) -> f64 {
    // Remove parentheses content and parse MW value
    let cleaned = value
        .split('(')
        .next()
        .unwrap_or(value)
        .replace(",", "");
    
    if cleaned == "-" || cleaned == "N/A" || cleaned.is_empty() {
        0.0
    } else {
        cleaned.parse().unwrap_or(0.0)
    }
}

fn clean_energy_type(energy_type: &str) -> String {
    // Simplify energy type names
    if energy_type.contains("民營電廠") {
        energy_type.replace("民營電廠-", "民營")
    } else if energy_type.contains("其它再生能源") {
        "其它再生能源".to_string()
    } else {
        energy_type.to_string()
    }
}

fn is_renewable(energy_type: &str) -> bool {
    matches!(energy_type, "風力" | "太陽能" | "水力" | "其它再生能源")
}

fn extract_plant_name(unit_name: &str) -> Option<String> {
    // Extract plant name from unit name (e.g., "台中#1" -> "台中")
    if let Some(pos) = unit_name.find('#') {
        Some(unit_name[..pos].to_string())
    } else if unit_name.contains("小計") {
        None
    } else {
        // For complex names, try to extract meaningful part
        let parts: Vec<&str> = unit_name.split(&['(', '[', '#'][..]).collect();
        Some(parts[0].trim().to_string())
    }
}

fn get_reserve_indicator_emoji(indicator: &str) -> &str {
    match indicator {
        "G" => "🟢", // Green (good)
        "Y" => "🟡", // Yellow (warning)
        "O" => "🟠", // Orange (concern)
        "R" => "🔴", // Red (critical)
        _ => "⚪",   // Unknown
    }
}

fn format_combined_power_message(data: &CombinedPowerData) -> String {
    let mut message = String::new();
    
    message.push_str("🔋 **台電即時電力資訊** 🔋\n\n");
    
    // Load data section (if available)
    if let Some(load_data) = &data.load_data {
        message.push_str("⚡ **電力供需資訊**\n");
        message.push_str(&format!("📊 **目前用電量**: {:.1} 萬瓩\n", load_data.current_load));
        message.push_str(&format!("📈 **目前使用率**: {:.1}%\n", load_data.current_util_rate));
        message.push_str(&format!("🔌 **預估今日最大供電能力**: {:.1} 萬瓩\n", load_data.forecast_max_supply_capacity));
        message.push_str(&format!("⬆️ **預估今日最高用電**: {:.1} 萬瓩\n", load_data.forecast_peak_demand_load));
        message.push_str(&format!("🔋 **預估今日尖峰備轉容量**: {:.1} 萬瓩\n", load_data.forecast_peak_reserve_capacity));
        message.push_str(&format!("{} **預估今日尖峰備轉容量率**: {:.2}%\n", 
            get_reserve_indicator_emoji(&load_data.forecast_peak_reserve_indicator), 
            load_data.forecast_peak_reserve_rate));
        message.push_str(&format!("🕐 **預估尖峰用電時段**: {}\n", load_data.forecast_peak_hour_range));
        message.push_str(&format!("📅 **資料更新時間**: {}\n\n", load_data.publish_time));
        
        // Yesterday's data
        message.push_str("📊 **昨日電力資訊**\n");
        message.push_str(&format!("🔌 **最大供電能力**: {:.1} 萬瓩\n", load_data.yesterday_max_supply_capacity));
        message.push_str(&format!("⬆️ **尖峰用電量**: {:.1} 萬瓩\n", load_data.yesterday_peak_demand_load));
        message.push_str(&format!("🔋 **尖峰備轉容量**: {:.1} 萬瓩\n", load_data.yesterday_peak_reserve_capacity));
        message.push_str(&format!("{} **尖峰備轉容量率**: {:.2}%\n\n", 
            get_reserve_indicator_emoji(&load_data.yesterday_peak_reserve_indicator),
            load_data.yesterday_peak_reserve_rate));
        
        // Real-time peak data
        if load_data.real_hour_max_supply_capacity > 0.0 {
            message.push_str("⏰ **即時尖峰資訊**\n");
            message.push_str(&format!("🔌 **即時最大供電能力**: {:.1} 萬瓩\n", load_data.real_hour_max_supply_capacity));
            message.push_str(&format!("🕰️ **尖峰時間**: {}\n\n", load_data.real_hour_peak_time));
        }
    }
    
    // Power generation analysis section
    let analysis = &data.power_analysis;
    message.push_str("🏭 **發電機組資訊**\n");
    message.push_str(&format!("📅 **更新時間**: {}\n", analysis.update_time));
    message.push_str(&format!("⚡ **總發電量**: {:.1} MW\n", analysis.total_generation));
    message.push_str(&format!("🔄 **裝置容量**: {:.1} MW\n", analysis.estimated_max_generation));
    message.push_str(&format!("📊 **發電占比**: {:.1}%\n\n", 
        (analysis.total_generation / analysis.estimated_max_generation) * 100.0));
    
    message.push_str("🏭 **各能源發電量**:\n");
    let mut sorted_types: Vec<_> = analysis.generation_by_type.iter().collect();
    sorted_types.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    for (energy_type, generation) in sorted_types {
        message.push_str(&format!("   • {}: {:.1} MW\n", energy_type, generation));
    }
    
    message.push_str(&format!("\n🏆 **發電量最高電廠**: {} ({:.1} MW)\n", 
        analysis.top_plant.0, analysis.top_plant.1));
    message.push_str(&format!("🥇 **發電量最高機組**: {} ({:.1} MW)\n", 
        analysis.top_unit.0, analysis.top_unit.1));
    
    message.push_str("\n📋 **運轉狀態統計**:\n");
    message.push_str(&format!("   🌱 環保限制/運轉限制: {} 部\n", analysis.environmental_restrictions));
    message.push_str(&format!("   🔧 歲修/檢修: {} 部\n", analysis.maintenance_count));
    message.push_str(&format!("   ⚠️ 故障: {} 部\n", analysis.fault_count));
    
    message.push_str(&format!("\n🌿 **再生能源占比**: {:.1}%\n", analysis.renewable_ratio));
    message.push_str(&format!("🏢 **民營電廠+購電占比**: {:.1}%\n", analysis.private_ratio));
    
    message.push_str("\n📊 資料來源: [台電公司開放資料](<https://data.gov.tw/dataset/8931>)");
    message.push_str("\n⚠️本資料可能會有錯誤或延遲，造成損失與我們無關");
    
    message
}

#[tokio::main]
async fn main() {
    // Get environment variables
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment");
    let channel_id = env::var("CHANNEL_ID")
        .expect("Expected a channel ID in the environment")
        .parse::<u64>()
        .expect("Invalid channel ID");
    
    // Set gateway intents
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    
    // Create a new instance of the Client
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            channel_id: ChannelId::new(channel_id),
        })
        .await
        .expect("Err creating client");
    
    // Start bot
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}