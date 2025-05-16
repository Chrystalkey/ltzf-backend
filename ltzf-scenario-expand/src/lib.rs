use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[serde(rename_all="lowercase")]
pub enum ScenarioType{
    Vorgang,
    Sitzung
}

#[derive(Deserialize, Debug)]
struct TestLoader {
    #[serde(rename="type")]
    tp: ScenarioType,
    #[serde(default)]
    context: Vec<Value>,
    object: Value,
    #[serde(default)]
    result: Vec<Value>,
    shouldfail: Option<bool>,
}

#[derive(Serialize)]
pub struct Scenario{
    #[serde(rename="type")]
    pub tp: ScenarioType,
    pub context: Vec<Value>,
    pub object: Value,
    pub result: Vec<Value>,
    pub shouldfail: bool,
}
impl Scenario{
    pub fn load(path: &str) -> anyhow::Result<Self>{
        let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read from path {}", path))?;
        let parsed: TestLoader = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse file with path: {}", path))?;
        if parsed.tp != ScenarioType::Vorgang {
            return Err(anyhow::anyhow!("Received Scenario of type {:?}, not of expected type Vorgang", parsed.tp));
        }
        let context = {
            let mut processed_context = Vec::with_capacity(parsed.context.len());
            for ct_val in &parsed.context{
                processed_context.push(recursive_merge(&parsed.object, ct_val));
            }
            processed_context
        };
        let result = {
            let mut processed_results = Vec::with_capacity(parsed.result.len());
            for rs_val in &parsed.result{
                processed_results.push(recursive_merge(&parsed.object, rs_val))
            }
            processed_results
        };
        Ok(
            Scenario{
                tp: parsed.tp,
                result,
                context,
                object: parsed.object,
                shouldfail: parsed.shouldfail.unwrap_or(false),
            }
        )
    }
    pub fn write_to(&self) -> anyhow::Result<()>{
        serde_json::to_string(&self)?;
        Ok(())
    }
}

fn recursive_merge(base: &Value, overlay: &Value) -> Value {
    inner_recursive_merge(base, overlay)
}

fn inner_recursive_merge(base: &Value, overlay: &Value) -> Value {
    match (base, overlay) {
        (Value::Object(bs), Value::Object(ov)) => {
            let mut result = bs.clone();
            if ov.is_empty() {
                return base.clone();
            } else {
                for (key, ov_val) in ov {
                    if result.contains_key(key){
                        result[key] = inner_recursive_merge(&result[key], ov_val);
                    }else{
                        result.insert(key.clone(), ov_val.clone());
                    }
                }
            }
            Value::Object(result)
        },
        (Value::Array(bs), Value::Array(ov)) =>{
            if ov.is_empty(){
                overlay.clone()
            }else{
                let mut bsidx = 0;
                let mut array = vec![];
                for elem in ov.iter() {
                    // if the ov array length is over the base array length, just add whatever is in there
                    if bsidx >= bs.len(){
                        array.push(elem.clone());
                        continue;
                    }
                    
                    // here idx is well defined for bs
                    if elem.is_null() {
                        bsidx += 1;
                        continue;
                    }else if let Value::Object(x) = elem {
                        if  x.is_empty() {
                            array.push(bs[bsidx].clone());
                        } else {
                            array.push(inner_recursive_merge(&bs[bsidx], elem));
                        }
                    } else {
                        array.push(elem.clone());
                    }
                    bsidx += 1;
                }
                if (bsidx-1) < bs.len(){
                    array.extend_from_slice(&bs[bsidx..])
                }
                Value::Array(array)
            }
        }
        (_, _) => {
            overlay.clone()
        }
    }
}

#[cfg(test)]
mod tests{
    use serde_json::Value;
    use serde::Deserialize;
    use super::*;

    #[derive(Deserialize)]
    struct TestingRig{
        base: Value,
        overlay: Value,
        expected: Value
    }
    fn build_test_string(path: &str) -> (String, TestingRig) {
        let tr: TestingRig = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        (format!("{{\"type\": \"vorgang\", \"context\": [{}], \"object\": {}, \"result\": [], \"shouldfail\": false}}", 
        tr.overlay.to_string(), tr.base.to_string()), tr)
    }
    #[test]
    fn testing() {
        for test in std::fs::read_dir("tests").unwrap(){
            let fname = test.unwrap().file_name().into_string().unwrap();
            if fname.starts_with("test_"){
                let test = build_test_string(&format!("tests/{fname}"));
                std::fs::write("test_lse.json", test.0).unwrap();
                let scenario = Scenario::load("test_lse.json").unwrap();
                std::fs::remove_file("test_lse.json").unwrap();
                assert_eq!(scenario.context[0], test.1.expected);
            }
        }
    }
}