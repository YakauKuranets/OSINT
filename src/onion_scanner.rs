use crate::sandbox_runner;

pub struct OnionScanner;

impl OnionScanner {
    pub fn new() -> Self {
        OnionScanner
    }

    /// Ищет запрос в дарквебе через Ahmia.fi
    pub async fn search_ahmia(&self, query: &str) -> Vec<String> {
        let url = format!(
            "http://juhanurmihxlp77nkq76byazcldy2hlmovfu2epvl5ankdibsot4csyd.onion/search/?q={}",
            query
        );
        let headers = vec![("User-Agent", "Mozilla/5.0")];
        let body = sandbox_runner::execute_ephemeral(&url, "GET", &headers).unwrap_or_default();
        self.parse_links(&body)
    }

    fn parse_links(&self, html: &str) -> Vec<String> {
        let mut links = Vec::new();
        for line in html.lines() {
            if line.contains("href=\"http") && line.contains(".onion") {
                let start = line.find("href=\"").unwrap() + 6;
                let end = line[start..].find('"').unwrap() + start;
                links.push(line[start..end].to_string());
            }
        }
        links
    }
}