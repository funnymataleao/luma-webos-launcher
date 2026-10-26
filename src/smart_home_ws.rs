//! Live Home Assistant WebSocket connection, registry room names, and services.
use crate::smart_home::{Entity, Update, parse_states};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{self, Message, stream::MaybeTlsStream};

struct Registry {
    area_names: HashMap<String, String>,
    device_areas: HashMap<String, String>,
    entity_areas: HashMap<String, String>,
    entity_devices: HashMap<String, String>,
    /// entity_category diagnostic/config, hidden_by or disabled_by.
    hidden: std::collections::HashSet<String>,
}
impl Registry {
    fn new() -> Self {
        Self {
            area_names: HashMap::new(),
            device_areas: HashMap::new(),
            entity_areas: HashMap::new(),
            entity_devices: HashMap::new(),
            hidden: std::collections::HashSet::new(),
        }
    }
    fn update(&mut self, id: u64, list: &Value) {
        let Some(items) = list.as_array() else {
            return;
        };
        match id {
            3 => {
                self.area_names.clear();
                for item in items {
                    if let (Some(id), Some(name)) =
                        (item["area_id"].as_str(), item["name"].as_str())
                    {
                        self.area_names.insert(id.into(), name.into());
                    }
                }
            }
            4 => {
                self.device_areas.clear();
                for item in items {
                    if let (Some(id), Some(area)) = (item["id"].as_str(), item["area_id"].as_str())
                    {
                        self.device_areas.insert(id.into(), area.into());
                    }
                }
            }
            5 => {
                self.entity_areas.clear();
                self.entity_devices.clear();
                self.hidden.clear();
                for item in items {
                    if let Some(id) = item["entity_id"].as_str() {
                        if !item["entity_category"].is_null()
                            || !item["hidden_by"].is_null()
                            || !item["disabled_by"].is_null()
                        {
                            self.hidden.insert(id.into());
                        }
                        if let Some(area) = item["area_id"].as_str() {
                            self.entity_areas.insert(id.into(), area.into());
                        }
                        if let Some(device) = item["device_id"].as_str() {
                            self.entity_devices.insert(id.into(), device.into());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn attach_rooms(&self, entities: &mut [Entity]) {
        for entity in entities {
            entity.hidden = self.hidden.contains(&entity.id);
            let area = self.entity_areas.get(&entity.id).or_else(|| {
                self.entity_devices
                    .get(&entity.id)
                    .and_then(|id| self.device_areas.get(id))
            });
            if let Some(name) = area.and_then(|id| self.area_names.get(id)) {
                entity.area = name.clone();
            }
        }
    }
}
#[derive(Debug)]
struct Call {
    entity_id: String,
    service: String,
    data: Value,
}
pub struct LiveService {
    stop: Option<Arc<AtomicBool>>,
    commands: Option<Sender<Call>>,
    online: Arc<AtomicBool>,
}
impl LiveService {
    pub fn new() -> Self {
        Self {
            stop: None,
            commands: None,
            online: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn connected(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }
    pub fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.commands = None;
        self.online.store(false, Ordering::Relaxed);
    }
    pub fn connect(&mut self, server: String, token: String, updates: Sender<Update>) {
        self.stop();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.stop = Some(stop.clone());
        self.commands = Some(tx);
        let online = self.online.clone();
        thread::spawn(move || run(server, token, updates, rx, stop, online));
    }
    pub fn call(&self, entity_id: String, service: String, data: Value) -> bool {
        if !self.connected() {
            return false;
        }
        self.commands.as_ref().is_some_and(|tx| {
            tx.send(Call {
                entity_id,
                service,
                data,
            })
            .is_ok()
        })
    }
}
fn socket_address(server: &str) -> String {
    if let Some(rest) = server.strip_prefix("https://") {
        format!("wss://{rest}/api/websocket")
    } else if let Some(rest) = server.strip_prefix("http://") {
        format!("ws://{rest}/api/websocket")
    } else {
        String::new()
    }
}
fn write_json(
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    value: Value,
) -> bool {
    socket.send(Message::Text(value.to_string().into())).is_ok()
}
fn timeout_socket(socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>) {
    match socket.get_mut() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(Some(Duration::from_millis(350)));
        }
        MaybeTlsStream::Rustls(s) => {
            let _ = s.sock.set_read_timeout(Some(Duration::from_millis(350)));
        }
        _ => {}
    }
}
fn update_entities(data: &Value, entities: &mut HashMap<String, Value>) -> bool {
    let Some(map) = data.as_object() else {
        return false;
    };
    let mut changed = false;
    if let Some(added) = map.get("a").and_then(Value::as_object) {
        for (id, val) in added {
            let attrs = val.get("a").cloned().unwrap_or_else(|| json!({}));
            entities.insert(id.clone(),json!({"entity_id":id,"state":val.get("s").and_then(Value::as_str).unwrap_or("unknown"),"attributes":attrs}));
            changed = true;
        }
    }
    if let Some(modified) = map.get("c").and_then(Value::as_object) {
        for (id, delta) in modified {
            if let Some(entry) = entities.get_mut(id) {
                if let Some(state) = delta.get("s") {
                    entry["state"] = state.clone();
                }
                if let Some(attributes) = delta.get("a") {
                    if let Some(new) = attributes.get("+").and_then(Value::as_object) {
                        for (key, value) in new {
                            entry["attributes"][key] = value.clone();
                        }
                    }
                    if let Some(old) = attributes.get("-").and_then(Value::as_array) {
                        for key in old.iter().filter_map(Value::as_str) {
                            if let Some(attrs) = entry["attributes"].as_object_mut() {
                                attrs.remove(key);
                            }
                        }
                    }
                }
                changed = true;
            }
        }
    }
    if let Some(removed) = map.get("r").and_then(Value::as_array) {
        for id in removed.iter().filter_map(Value::as_str) {
            entities.remove(id);
            changed = true;
        }
    }
    changed
}
fn broadcast_entities(
    entities: &HashMap<String, Value>,
    rooms: &Registry,
    updates: &Sender<Update>,
) {
    let raw = Value::Array(entities.values().cloned().collect()).to_string();
    if let Ok(mut parsed) = parse_states(&raw) {
        rooms.attach_rooms(&mut parsed);
        let _ = updates.send(Update::States(parsed));
    }
}
fn run(
    server: String,
    token: String,
    updates: Sender<Update>,
    calls: Receiver<Call>,
    stop: Arc<AtomicBool>,
    online: Arc<AtomicBool>,
) {
    let address = socket_address(&server);
    let mut delay = 1u64;
    while !stop.load(Ordering::Relaxed) {
        let connection = tungstenite::connect(address.as_str());
        if let Ok((mut socket, _response)) = connection {
            delay = 1;
            timeout_socket(&mut socket);
            let mut authenticated = false;
            let mut entities = HashMap::new();
            let mut rooms = Registry::new();
            let mut next_id = 10u64;
            let mut last_ping = Instant::now();
            let mut last_receive = Instant::now();
            loop {
                if stop.load(Ordering::Relaxed) {
                    let _ = socket.close(None);
                    break;
                }
                if authenticated {
                    while let Ok(call) = calls.try_recv() {
                        if let Some((domain, _)) = call.entity_id.split_once('.') {
                            next_id += 1;
                            if !write_json(
                                &mut socket,
                                json!({"id":next_id,"type":"call_service","domain":domain,"service":call.service,"target":{"entity_id":call.entity_id},"service_data":call.data}),
                            ) {
                                break;
                            }
                        }
                    }
                    if last_ping.elapsed() >= Duration::from_secs(25) {
                        next_id += 1;
                        if !write_json(&mut socket, json!({"id":next_id,"type":"ping"})) {
                            break;
                        }
                        last_ping = Instant::now();
                    }
                }
                if authenticated && last_receive.elapsed() > Duration::from_secs(75) {
                    break;
                }
                let incoming = match socket.read() {
                    Ok(message) => message,
                    Err(tungstenite::Error::Io(ref e))
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                        ) =>
                    {
                        continue;
                    }
                    Err(_) => break,
                };
                last_receive = Instant::now();
                let Message::Text(text) = incoming else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
                match kind {
                    "auth_required" => {
                        if !write_json(&mut socket, json!({"type":"auth","access_token":token})) {
                            break;
                        }
                    }
                    "auth_ok" => {
                        authenticated = true;
                        online.store(true, Ordering::Relaxed);
                        let _ = updates.send(Update::Connected);
                        for (id, kind) in [
                            (1, "subscribe_entities"),
                            (2, "get_states"),
                            (3, "config/area_registry/list"),
                            (4, "config/device_registry/list"),
                            (5, "config/entity_registry/list"),
                        ] {
                            if !write_json(&mut socket, json!({"id":id,"type":kind})) {
                                break;
                            }
                        }
                    }
                    "auth_invalid" => {
                        let _ = updates.send(Update::Error(
                            "Home Assistant authentication expired".into(),
                        ));
                        break;
                    }
                    "event" if value.get("id").and_then(Value::as_u64) == Some(1) => {
                        if update_entities(&value["event"], &mut entities) {
                            broadcast_entities(&entities, &rooms, &updates);
                        }
                    }
                    "result" => {
                        let id = value.get("id").and_then(Value::as_u64).unwrap_or(0);
                        if value.get("success").and_then(Value::as_bool) == Some(true) {
                            if id == 2 {
                                if let Some(list) = value["result"].as_array() {
                                    entities.clear();
                                    for item in list {
                                        if let Some(id) = item["entity_id"].as_str() {
                                            entities.insert(id.into(), item.clone());
                                        }
                                    }
                                }
                            } else if (3..=5).contains(&id) {
                                rooms.update(id, &value["result"]);
                            }
                            if (2..=5).contains(&id) {
                                broadcast_entities(&entities, &rooms, &updates);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        online.store(false, Ordering::Relaxed);
        if !stop.load(Ordering::Relaxed) {
            let _ = updates.send(Update::Offline);
            for _ in 0..delay * 10 {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            // Home Assistant is on the LAN: after a restart, come back within seconds.
            delay = (delay * 2).min(10);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn websocket_endpoint_and_delta() {
        assert_eq!(
            socket_address("https://ha.local"),
            "wss://ha.local/api/websocket"
        );
        assert_eq!(
            socket_address("http://ha.local:8123"),
            "ws://ha.local:8123/api/websocket"
        );
        let mut data = HashMap::new();
        assert!(update_entities(
            &json!({"a":{"light.desk":{"s":"on","a":{"brightness":200}}}}),
            &mut data
        ));
        assert_eq!(data["light.desk"]["state"], "on");
        assert!(update_entities(
            &json!({"c":{"light.desk":{"s":"off","a":{"+":{"brightness":0}}}}}),
            &mut data
        ));
        assert_eq!(data["light.desk"]["attributes"]["brightness"], 0);
        assert!(update_entities(&json!({"r":["light.desk"]}), &mut data));
        assert!(data.is_empty());
    }
    #[test]
    fn rooms_from_registries() {
        let mut r = Registry::new();
        r.update(3, &json!([{"area_id":"lounge","name":"Living Room"}]));
        r.update(4, &json!([{"id":"d1","area_id":"lounge"}]));
        r.update(5, &json!([{"entity_id":"light.desk","device_id":"d1"}]));
        let mut entities = parse_states(
            &json!([{"entity_id":"light.desk","state":"on","attributes":{}}]).to_string(),
        )
        .unwrap();
        r.attach_rooms(&mut entities);
        assert_eq!(entities[0].area, "Living Room");
        assert!(!entities[0].hidden);
    }
    #[test]
    fn registry_marks_diagnostic_hidden_and_disabled() {
        let mut r = Registry::new();
        r.update(
            5,
            &json!([
                {"entity_id":"switch.led","entity_category":"config"},
                {"entity_id":"sensor.rssi","entity_category":"diagnostic"},
                {"entity_id":"light.old","disabled_by":"user"},
                {"entity_id":"light.attic","hidden_by":"user"},
                {"entity_id":"light.desk","entity_category":null}
            ]),
        );
        let mut entities = parse_states(
            &json!([
                {"entity_id":"switch.led","state":"on"},
                {"entity_id":"sensor.rssi","state":"-60"},
                {"entity_id":"light.old","state":"off"},
                {"entity_id":"light.attic","state":"off"},
                {"entity_id":"light.desk","state":"on"}
            ])
            .to_string(),
        )
        .unwrap();
        r.attach_rooms(&mut entities);
        let hidden: Vec<_> = entities
            .iter()
            .filter(|e| e.hidden)
            .map(|e| &e.id)
            .collect();
        assert_eq!(
            hidden,
            ["switch.led", "sensor.rssi", "light.old", "light.attic"]
        );
    }
}
