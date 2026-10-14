//! Minimal XML-RPC layer for rtorrent's SCGI endpoint.
//!
//! Only what rtorrent needs: method calls with string/integer/array params
//! and parsing of `methodResponse` values (including nested arrays) plus
//! `<fault>` detection. Deliberately not a general XML-RPC implementation.

/// A parsed XML-RPC value (the subset rtorrent uses).
#[derive(Debug, Clone, PartialEq)]
pub enum RpcValue {
    Str(String),
    Int(i64),
    Array(Vec<RpcValue>),
}

impl RpcValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            RpcValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            RpcValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[RpcValue]> {
        match self {
            RpcValue::Array(items) => Some(items),
            _ => None,
        }
    }
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn encode_value(value: &RpcValue, out: &mut String) {
    match value {
        RpcValue::Str(s) => {
            out.push_str("<value><string>");
            out.push_str(&escape_xml(s));
            out.push_str("</string></value>");
        }
        RpcValue::Int(i) => {
            out.push_str(&format!("<value><i8>{i}</i8></value>"));
        }
        RpcValue::Array(items) => {
            out.push_str("<value><array><data>");
            for item in items {
                encode_value(item, out);
            }
            out.push_str("</data></array></value>");
        }
    }
}

/// Build a complete XML-RPC `methodCall` document.
pub fn build_call(method: &str, params: &[RpcValue]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><methodCall><methodName>",
    );
    out.push_str(&escape_xml(method));
    out.push_str("</methodName><params>");
    for param in params {
        out.push_str("<param>");
        encode_value(param, &mut out);
        out.push_str("</param>");
    }
    out.push_str("</params></methodCall>");
    out
}

fn parse_value(node: roxmltree::Node) -> Result<RpcValue, String> {
    // A <value> may either wrap a typed element or contain raw text.
    let children: Vec<roxmltree::Node> = node.children().filter(|c| c.is_element()).collect();
    if children.is_empty() {
        // Untyped value: treat as string.
        return Ok(RpcValue::Str(node.text().unwrap_or("").to_string()));
    }
    let typed = children[0];
    let text = typed.text().unwrap_or("").to_string();
    match typed.tag_name().name() {
        "string" => Ok(RpcValue::Str(text)),
        "i4" | "i8" | "int" => text
            .parse::<i64>()
            .map(RpcValue::Int)
            .map_err(|e| format!("rtorrent: bad integer value '{text}': {e}")),
        "double" => text
            .parse::<f64>()
            .map(|f| RpcValue::Int(f as i64))
            .map_err(|e| format!("rtorrent: bad double value '{text}': {e}")),
        "array" => {
            let mut items = Vec::new();
            for data in typed.children().filter(|c| c.is_element() && c.tag_name().name() == "data") {
                for value in data.children().filter(|c| c.is_element() && c.tag_name().name() == "value") {
                    items.push(parse_value(value)?);
                }
            }
            Ok(RpcValue::Array(items))
        }
        other => Err(format!("rtorrent: unsupported XML-RPC value type '{other}'")),
    }
}

/// Parse a `methodResponse` document into its single return value.
pub fn parse_response(xml: &str) -> Result<RpcValue, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| format!("rtorrent: invalid XML-RPC response: {e}"))?;
    let response = doc
        .root_element()
        .children()
        .find(|n| n.is_element())
        .ok_or_else(|| "rtorrent: empty methodResponse".to_string())?;

    match response.tag_name().name() {
        "fault" => {
            // <fault><value><struct><member><name>faultString</name><value>...
            let message = response
                .descendants()
                .find(|n| {
                    n.tag_name().name() == "member"
                        && n.children()
                            .any(|c| c.tag_name().name() == "name" && c.text() == Some("faultString"))
                })
                .and_then(|member| {
                    member
                        .descendants()
                        .find(|n| n.tag_name().name() == "string")
                        .and_then(|n| n.text().map(|t| t.to_string()))
                })
                .unwrap_or_else(|| "unknown fault".to_string());
            Err(format!("rtorrent: {message}"))
        }
        "params" => {
            let param = response
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "param")
                .ok_or_else(|| "rtorrent: methodResponse without params".to_string())?;
            let value = param
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "value")
                .ok_or_else(|| "rtorrent: param without value".to_string())?;
            parse_value(value)
        }
        other => Err(format!("rtorrent: unexpected response element '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_call_escapes_params() {
        let call = build_call(
            "load.raw_start",
            &[RpcValue::Str(String::new()), RpcValue::Str("a<b>&c".into())],
        );
        assert!(call.contains("<methodName>load.raw_start</methodName>"));
        assert!(call.contains("<string>a&lt;b&gt;&amp;c</string>"));
    }

    #[test]
    fn test_parse_multicall_response() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <methodResponse><params><param><value><array><data>
          <value><array><data>
            <value><string>BF4A97C0</string></value>
            <value><string>Tokyo Story</string></value>
            <value><i8>100</i8></value>
          </data></array></value>
          <value><array><data>
            <value><string>0DEAD0</string></value>
            <value><string>Second</string></value>
            <value><i8>50</i8></value>
          </data></array></value>
        </data></array></value></param></params></methodResponse>"#;

        let parsed = parse_response(xml).unwrap();
        let rows = parsed.as_array().unwrap();
        assert_eq!(rows.len(), 2);
        let first = rows[0].as_array().unwrap();
        assert_eq!(first[0].as_str(), Some("BF4A97C0"));
        assert_eq!(first[1].as_str(), Some("Tokyo Story"));
        assert_eq!(first[2].as_int(), Some(100));
    }

    #[test]
    fn test_parse_fault() {
        let xml = r#"<?xml version="1.0"?>
        <methodResponse><fault><value><struct>
          <member><name>faultCode</name><value><i4>1</i4></value></member>
          <member><name>faultString</name><value><string>no connection to rtorrent</string></value></member>
        </struct></value></fault></methodResponse>"#;
        let err = parse_response(xml).unwrap_err();
        assert!(err.contains("no connection to rtorrent"), "{err}");
    }
}
