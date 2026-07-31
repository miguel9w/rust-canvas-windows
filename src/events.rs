use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex;

/// Um evento emitido por um widget (via appBus) e registrado pelo app.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EventRecord {
    /// Timestamp (epoch millis)
    pub ts: u64,
    /// Título da janela que emitiu
    pub window: String,
    /// Nome do evento (ex: "corp:vendas")
    pub evt: String,
    /// Payload do evento
    pub data: Value,
}

/// Log central de eventos do appBus: o agente consulta via HTTP
/// (`GET /events` e `GET /events?since=<ts>`).
pub struct EventLog {
    inner: Mutex<EventLogInner>,
}

struct EventLogInner {
    records: VecDeque<EventRecord>,
}

/// Máximo de eventos retidos no buffer
const MAX_RECORDS: usize = 100;

impl EventLog {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(EventLogInner {
                records: VecDeque::new(),
            }),
        }
    }

    /// Registra um evento (e limita o buffer).
    pub fn push(&self, window: &str, payload: &str) {
        // payload: {"evt": "...", "data": ...}
        let (evt, data) = serde_json::from_str::<Value>(payload)
            .ok()
            .map(|v| {
                (
                    v.get("evt")
                        .and_then(|e| e.as_str())
                        .unwrap_or("?")
                        .to_string(),
                    v.get("data").cloned().unwrap_or(Value::Null),
                )
            })
            .unwrap_or_else(|| ("?".into(), Value::Null));

        let record = EventRecord {
            ts: now_millis(),
            window: window.to_string(),
            evt,
            data,
        };

        let mut inner = self.inner.lock().unwrap();
        inner.records.push_back(record);
        while inner.records.len() > MAX_RECORDS {
            inner.records.pop_front();
        }
    }

    /// Últimos `n` eventos (do mais novo para o mais antigo).
    pub fn recent(&self, n: usize) -> Vec<EventRecord> {
        let inner = self.inner.lock().unwrap();
        inner.records.iter().rev().take(n).cloned().collect()
    }

    /// Eventos com `ts >= since_ts` (mais novo primeiro) — polling incremental.
    pub fn since(&self, since_ts: u64, n: usize) -> Vec<EventRecord> {
        let inner = self.inner.lock().unwrap();
        inner
            .records
            .iter()
            .rev()
            .filter(|r| r.ts >= since_ts)
            .take(n)
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().records.clear();
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
