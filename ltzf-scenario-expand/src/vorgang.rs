use openapi::models::*;
use json_serde::{Value};

#[derive(Deserialize, Debug)]
struct VGTestLoader{
    #[serde(rename="type")]
    tp: super::ScenarioType,
    #[serde(default)]
    context: Vec<Value>,
    object: Value,
    #[serde(default)]
    result: Vec<Value>,
    shouldfail: Option<bool>,
}

pub struct VorgangTestScenario{
    pub context: Vec<Vorgang>,
    pub object: Vorgang,
    pub result: Vec<Vorgang>,
    pub shouldfail: bool,
}

impl crate::Scenario for VorgangTestScenario{
    type ObjectType = Vorgang;
    fn load(path: &str) -> anyhow::Result<Self>{
        let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read from path {}", path))?;
        let parsed: VGTestLoader = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse file with path: {}", path))?;
        if parsed.tp != super::ScenarioType::Vorgang{
            return Err(anyhow::anyhow!("Received Scenario of type {}, not of expected type Vorgang", parsed.tp));
        }
        let context = {
            let mut processed_context = Vec::with_capacity(parsed.context.len());
            for ct_val in &parsed.context{
                processed_context.push(vorgang_replace(&parsed.object, ct_val));
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
            VorgangTestScenario{
                result,
                context,
                object: parsed.object,
                shouldfail: parsed.shouldfail.unwrap_or(false),
            }
        )
    }
}
fn recursive_merge(base: &Value, overlay: &Value) -> Vorgang{
    inner_recursive_merge(base, overlay).parse()
}

// takes the values of one and replaces any values existing in two with them
fn inner_recursive_merge(base: &Value, overlay: &Value) -> Value{
    match (base, overlay) {
        (Value::Object(bs), Value::Object(ov)) => {
            let mut result = bs.clone();
            if ov.is_null() || ov.is_empty() {
                return result;
            } else {
                for (key, ov_val) in ov {
                    result[key] = inner_recursive_merge(result[key], ov_val)
                }
            }
            Value::Object(result)
        },
        (Value::Array(bs), Value::Array(ov)) =>{
            let mut array = vec![];
            let mut idx = 0;
            for elem in ov.iter() {
                if elem.is_null(){
                    continue;
                }else if let Some(Value::Object(x)) {
                    if  x.is_empty(){
                        array.push(bs[idx]);
                    }else{
                        array.push(Value::Object(x));
                    }
                }else{
                    array.push(elem);
                }
                idx += 1;
            }
        }
        // TODO
        (_, _) => {
            if !ov.is_null() {
                overlay
            }else{
                base
            }
        }
    }
}