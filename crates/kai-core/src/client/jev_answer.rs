//! Typed answers from a decision model (hosted JEV or local jevos), parsed
//! tolerantly: field names differ between the hosted endpoints and jevos, and
//! jevos' exact choice/score field names are not documented. Anything that
//! cannot be read is left `None`, never guessed.

use std::collections::BTreeMap;

/// One typed answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Noul(f64),
    Boolean {
        probability: f64,
    },
    /// The most probable option, the per-option probabilities (empty when the
    /// server sent none) and the server's confidence (if it sent one, else
    /// the chosen option's probability, if known).
    Choice {
        option: String,
        probabilities: BTreeMap<String, f64>,
        confidence: Option<f64>,
    },
    /// The expected level: a number (`value`) and/or a level name (`level`),
    /// per-level probabilities and a confidence, as far as the server sent
    /// them.
    Score {
        value: Option<f64>,
        level: Option<String>,
        probabilities: BTreeMap<String, f64>,
        confidence: Option<f64>,
    },
}

const OPTION_KEYS: &[&str] = &["choice", "answer", "option", "value", "label"];
const PROB_KEYS: &[&str] = &["probabilities", "scores", "distribution", "probs"];

/// A per-option map from `{"a":0.7,"b":0.3}` or `[{"option":"a","probability":0.7}]`.
fn prob_map(v: &serde_json::Value) -> BTreeMap<String, f64> {
    let Some(raw) = PROB_KEYS.iter().map(|k| &v[*k]).find(|x| !x.is_null()) else {
        return BTreeMap::new();
    };
    if let Some(o) = raw.as_object() {
        return o
            .iter()
            .filter_map(|(k, p)| p.as_f64().map(|p| (k.clone(), p)))
            .collect();
    }
    raw.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|it| {
                    let name = ["option", "label", "choice", "level", "name"]
                        .iter()
                        .find_map(|k| it[*k].as_str())?;
                    let p = ["probability", "p", "score", "value"]
                        .iter()
                        .find_map(|k| it[*k].as_f64())?;
                    Some((name.to_string(), p))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn argmax(m: &BTreeMap<String, f64>) -> Option<String> {
    m.iter()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(k, _)| k.clone())
}

impl Answer {
    /// P(yes) for `noul`/`boolean` answers.
    pub fn probability(&self) -> Option<f64> {
        match self {
            Answer::Noul(p) | Answer::Boolean { probability: p } => Some(*p),
            _ => None,
        }
    }

    /// The chosen option of a `choice` answer.
    pub fn option(&self) -> Option<&str> {
        match self {
            Answer::Choice { option, .. } => Some(option),
            _ => None,
        }
    }

    /// Confidence of a choice/score answer.
    pub fn confidence(&self) -> Option<f64> {
        match self {
            Answer::Choice { confidence, .. } | Answer::Score { confidence, .. } => *confidence,
            _ => None,
        }
    }

    /// How far the top option leads the runner-up. With a full distribution
    /// that is `p1 - p2`; with only the top probability `p` it is the lower
    /// bound `2p - 1` (the runner-up holds at most `1 - p`). `None` when
    /// nothing numeric is known.
    pub fn margin(&self) -> Option<f64> {
        let Answer::Choice {
            probabilities,
            confidence,
            option,
        } = self
        else {
            return None;
        };
        let mut ps: Vec<f64> = probabilities.values().copied().collect();
        ps.sort_by(|a, b| b.total_cmp(a));
        match ps.as_slice() {
            [a, b, ..] => Some(a - b),
            [a] => Some(2.0 * a - 1.0),
            [] => probabilities
                .get(option)
                .copied()
                .or(*confidence)
                .map(|p| 2.0 * p - 1.0),
        }
    }

    /// Parse one answer object; tolerant of field-name variants. A missing
    /// `type` is inferred from the fields present.
    pub fn parse(v: &serde_json::Value) -> Option<Self> {
        let num = |keys: &[&str]| keys.iter().find_map(|k| v[*k].as_f64());
        let ty = v["type"].as_str().unwrap_or_else(|| {
            if v["noul"].is_number() {
                "noul"
            } else {
                "choice"
            }
        });
        match ty {
            "noul" => num(&["noul", "probability", "value"]).map(Answer::Noul),
            "boolean" => {
                let p = num(&["probability", "noul", "p"]).or_else(|| {
                    v["value"]
                        .as_bool()
                        .or_else(|| v["answer"].as_bool())
                        .map(|b| if b { 1.0 } else { 0.0 })
                })?;
                Some(Answer::Boolean { probability: p })
            }
            "choice" => {
                let probabilities = prob_map(v);
                let option = OPTION_KEYS
                    .iter()
                    .find_map(|k| v[*k].as_str().map(str::to_string))
                    .or_else(|| argmax(&probabilities))?;
                let confidence =
                    num(&["confidence"]).or_else(|| probabilities.get(&option).copied());
                Some(Answer::Choice {
                    option,
                    probabilities,
                    confidence,
                })
            }
            "score" => {
                let probabilities = prob_map(v);
                let value = num(&["score", "value", "expected", "level"]);
                let level = ["level", "expected", "value", "score"]
                    .iter()
                    .find_map(|k| v[*k].as_str().map(str::to_string))
                    .or_else(|| value.is_none().then(|| argmax(&probabilities)).flatten());
                if value.is_none() && level.is_none() {
                    return None;
                }
                let confidence = num(&["confidence"])
                    .or_else(|| level.as_ref().and_then(|l| probabilities.get(l).copied()));
                Some(Answer::Score {
                    value,
                    level,
                    probabilities,
                    confidence,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn noul_boolean_and_old_shapes() {
        let n = Answer::parse(&json!({"type":"noul","noul":0.98})).unwrap();
        assert_eq!(n.probability(), Some(0.98));
        let b = Answer::parse(&json!({"type":"boolean","value":true})).unwrap();
        assert_eq!(b.probability(), Some(1.0));
        let c = Answer::parse(&json!({"type":"choice","choice":"save"})).unwrap();
        assert_eq!(c.option(), Some("save"));
        assert_eq!(c.confidence(), None);
        assert_eq!(c.margin(), None);
        assert!(Answer::parse(&json!({"type":"other"})).is_none());
        assert!(Answer::parse(&json!({"type":"choice"})).is_none());
    }

    #[test]
    fn choice_variants_and_margins() {
        let c = Answer::parse(&json!({"type":"choice","answer":"3",
            "probabilities":{"3":0.7,"5":0.2,"none":0.1},"confidence":0.66}))
        .unwrap();
        assert_eq!(c.option(), Some("3"));
        assert_eq!(c.confidence(), Some(0.66));
        assert!((c.margin().unwrap() - 0.5).abs() < 1e-9);
        // No option field: the argmax of the distribution; confidence from it.
        let c = Answer::parse(&json!({"type":"choice","scores":{"a":0.2,"b":0.8}})).unwrap();
        assert_eq!(c.option(), Some("b"));
        assert_eq!(c.confidence(), Some(0.8));
        // Array form.
        let c = Answer::parse(&json!({"choice":"x",
            "distribution":[{"option":"x","probability":0.9},{"option":"y","p":0.1}]}))
        .unwrap();
        assert!((c.margin().unwrap() - 0.8).abs() < 1e-9);
        // Only a confidence: margin lower bound 2p-1.
        let c = Answer::parse(&json!({"type":"choice","value":"x","confidence":0.75})).unwrap();
        assert!((c.margin().unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn score_variants() {
        let s = Answer::parse(&json!({"type":"score","score":7.5})).unwrap();
        assert!(matches!(s, Answer::Score { value: Some(v), .. } if v == 7.5));
        let s = Answer::parse(&json!({"type":"score","expected":"calm",
            "probabilities":{"calm":0.6,"annoyed":0.4}}))
        .unwrap();
        assert!(matches!(&s, Answer::Score { level: Some(l), .. } if l == "calm"));
        assert_eq!(s.confidence(), Some(0.6));
        let s = Answer::parse(&json!({"type":"score","probabilities":{"a":0.1,"b":0.9}})).unwrap();
        assert!(matches!(&s, Answer::Score { level: Some(l), .. } if l == "b"));
        assert!(Answer::parse(&json!({"type":"score"})).is_none());
    }
}
