use super::*;

fn files(dir: &str, entry: &str) -> BTreeMap<String, String> {
    topo_script::bundle::collect(&format!("{dir}/{entry}"), &mut |id| std::fs::read_to_string(id).ok()).unwrap_or_else(|e| panic!("{e}"))
}

fn examples() -> String {
    format!("{}/../../examples", env!("CARGO_MANIFEST_DIR"))
}

/// A session builds a model from its files, answers requests, and turns a
/// new measurement into a file for the host to write.
#[test]
fn session_builds_and_answers() {
    let dir = examples();
    let entry = format!("{dir}/garage-truss.ts");
    let mut s = Session::new(&entry, files(&dir, "garage-truss.ts"));
    let script = s.script().unwrap();
    s.rebuild(&script);
    let st = s.state();
    assert!(st.get("error").is_none(), "{st}");
    assert!(st["sheets"].as_array().unwrap().len() >= 2);
    assert!(st["fit"]["rms"].as_f64().is_some(), "measurements fitted");
    let svg = s.request("sheet", r#"{"sheet": 0}"#).unwrap();
    assert!(svg.starts_with("<svg") || svg.starts_with("<?xml"));
    let mesh: Value = serde_json::from_str(&s.request("mesh", "").unwrap()).unwrap();
    assert!(!mesh["members"].as_array().unwrap().is_empty());
    let v: Value = serde_json::from_str(&s.request("view3d", r#"{"az": 200, "el": 25}"#).unwrap()).unwrap();
    assert!(!v["segments"].as_array().unwrap().is_empty());
    assert!(s.request("nonsense", "").is_err());

    // A measurement: the sidecar's new text comes back to write.
    let path = mesh["members"][0]["path"].as_str().unwrap().to_string();
    let q = json!({ "kind": "length", "member": path, "how": "long" });
    let args = json!({ "name": "test length", "quantity": q, "value": "7' 6\"", "note": null }).to_string();
    let (w, line) = s.measure(&args).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(w.id, format!("{dir}/garage-truss.measured.ts"));
    assert!(line.starts_with("measured(\"test length\""), "{line}");
    assert!(w.text.contains(&line));
}

/// The scenario and exclusions change what is built.
#[test]
fn scenarios_and_exclusions() {
    let dir = format!("{}/benchmarks", examples());
    let entry = format!("{dir}/05-roof-takedown.ts");
    let mut s = Session::new(&entry, files(&dir, "05-roof-takedown.ts"));
    let script = s.script().unwrap();
    s.rebuild(&script);
    let combos = |s: &Session| s.state()["loads"]["combos"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert!(combos(&s).iter().any(|c| c.contains('S')));
    s.excluded = vec!["S".into()];
    let script = s.script().unwrap();
    s.rebuild(&script);
    assert!(!combos(&s).iter().any(|c| c.contains('S')), "{:?}", combos(&s));
    assert_eq!(s.version, 2);
}

#[test]
fn new_project_template_builds_clean() {
    let entry = "projects/test-shed.ts".to_string();
    let mut s = Session::new(&entry, BTreeMap::from([(entry.clone(), new_project("Test shed"))]));
    let script = s.script().unwrap();
    s.rebuild(&script);
    let b = s.built.as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.model.info.name, "Test shed");
    assert!(b.issues.is_empty(), "{:#?}", b.issues);
    assert_eq!(slug(" My Garage! (v2) "), "my-garage-v2");
}
