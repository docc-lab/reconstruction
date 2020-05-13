/// Stuff related to working with osprofiler
///
use redis::Commands;
use redis::Connection;
use uuid::Uuid;

use pythia_common::OSProfilerEnum;
use pythia_common::OSProfilerSpan;

use crate::settings::Settings;

pub struct OSProfilerReader {
    connection: Connection,
}

impl OSProfilerReader {
    pub fn from_settings(settings: &Settings) -> OSProfilerReader {
        let redis_url = &settings.redis_url;
        let client = redis::Client::open(&redis_url[..]).unwrap();
        let con = client.get_connection().unwrap();
        OSProfilerReader { connection: con }
    }

    /// Public wrapper for get_matches_ that accepts string input and does not return RedisResult
    pub fn get_matches(&mut self, span_id: &str) -> Vec<OSProfilerSpan> {
        match Uuid::parse_str(span_id) {
            Ok(uuid) => self.get_matches_(&uuid).unwrap(),
            Err(_) => panic!("Malformed UUID as base id: {}", span_id),
        }
    }

    /// Get matching events from local redis instance
    fn get_matches_(&mut self, span_id: &Uuid) -> redis::RedisResult<Vec<OSProfilerSpan>> {
        let to_parse: String = match self
            .connection
            .get("osprofiler:".to_string() + &span_id.to_hyphenated().to_string())
        {
            Ok(to_parse) => to_parse,
            Err(_) => {
                return Ok(Vec::new());
            }
        };
        let mut result = Vec::new();
        for dict_string in to_parse[1..to_parse.len() - 1].split("}{") {
            match parse_field(&("{".to_string() + dict_string + "}")) {
                Ok(span) => {
                    result.push(span);
                }
                Err(e) => panic!("Problem while parsing {}: {}", dict_string, e),
            }
        }
        Ok(result)
    }
}

fn parse_field(field: &String) -> Result<OSProfilerSpan, String> {
    let result: OSProfilerSpan = match serde_json::from_str(field) {
        Ok(a) => a,
        Err(e) => {
            return Err(e.to_string());
        }
    };
    if result.name == "asynch_request" || result.name == "asynch_wait" {
        return match result.info {
            OSProfilerEnum::Annotation(_) => Ok(result),
            _ => {
                println!("{:?}", result);
                Err("".to_string())
            }
        };
    }
    Ok(result)
}