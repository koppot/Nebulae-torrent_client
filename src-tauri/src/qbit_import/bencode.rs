use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BVal {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<BVal>),
    Dict(BTreeMap<Vec<u8>, BVal>),
}

impl BVal {
    pub fn as_dict(&self) -> Option<&BTreeMap<Vec<u8>, BVal>> {
        match self {
            BVal::Dict(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<BVal>> {
        match self {
            BVal::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            BVal::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BVal::Bytes(b) => Some(b.as_slice()),
            _ => None,
        }
    }

    pub fn as_str_lossy(&self) -> Option<String> {
        match self {
            BVal::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }

    pub fn dict_get(&self, key: &str) -> Option<&BVal> {
        let dict = self.as_dict()?;
        dict.get(key.as_bytes())
    }

    pub fn dict_get_int(&self, key: &str) -> Option<i64> {
        self.dict_get(key)?.as_int()
    }

    pub fn dict_get_bytes(&self, key: &str) -> Option<&[u8]> {
        self.dict_get(key)?.as_bytes()
    }

    pub fn dict_get_str(&self, key: &str) -> Option<String> {
        self.dict_get(key)?.as_str_lossy()
    }

    pub fn dict_get_list(&self, key: &str) -> Option<&Vec<BVal>> {
        self.dict_get(key)?.as_list()
    }

    pub fn dict_get_dict(&self, key: &str) -> Option<&BTreeMap<Vec<u8>, BVal>> {
        self.dict_get(key)?.as_dict()
    }

    /// Extracts the root torrent name and relative file paths from metainfo bytes
    pub fn extract_torrent_files_and_name(raw_bytes: &[u8]) -> Option<(String, Vec<String>)> {
        let root = BencodeParser::new(raw_bytes).parse().ok()?;
        let info = root.dict_get("info")?;
        let name = info
            .dict_get_str("name")
            .unwrap_or_else(|| "Unknown".to_string());

        if let Some(files) = info.dict_get_list("files") {
            let mut paths = Vec::new();
            for f in files {
                if let Some(path_list) = f.dict_get_list("path") {
                    let parts: Vec<String> =
                        path_list.iter().filter_map(|p| p.as_str_lossy()).collect();
                    if !parts.is_empty() {
                        paths.push(format!("{}/{}", name, parts.join("/")));
                    }
                }
            }
            if !paths.is_empty() {
                return Some((name, paths));
            }
        }

        // Single-file torrent
        Some((name.clone(), vec![name]))
    }
}

pub struct BencodeParser<'a> {
    data: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> BencodeParser<'a> {
    const MAX_DEPTH: usize = 128;

    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            depth: 0,
        }
    }

    pub fn parse(mut self) -> Result<BVal, String> {
        let val = self.parse_val()?;
        Ok(val)
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn consume(&mut self) -> Option<u8> {
        if self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            Some(b)
        } else {
            None
        }
    }

    fn parse_val(&mut self) -> Result<BVal, String> {
        if self.depth > Self::MAX_DEPTH {
            return Err("Bencode recursion limit exceeded".to_string());
        }
        let b = self
            .peek()
            .ok_or_else(|| "Unexpected EOF in bencode".to_string())?;
        match b {
            b'i' => self.parse_int(),
            b'l' => self.parse_list(),
            b'd' => self.parse_dict(),
            b'0'..=b'9' => self.parse_bytes(),
            _ => Err(format!(
                "Invalid bencode token '{:02x}' at offset {}",
                b, self.pos
            )),
        }
    }

    fn parse_int(&mut self) -> Result<BVal, String> {
        self.consume(); // 'i'
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'e' {
                break;
            }
            self.consume();
        }
        let end_token = self
            .consume()
            .ok_or_else(|| "Unterminated integer in bencode".to_string())?;
        if end_token != b'e' {
            return Err("Expected 'e' at end of integer".to_string());
        }
        let raw = &self.data[start..self.pos - 1];
        let s = std::str::from_utf8(raw)
            .map_err(|e| format!("Invalid utf-8 in integer at {start}: {e}"))?;
        let val = s
            .parse::<i64>()
            .map_err(|e| format!("Invalid integer format '{s}' at {start}: {e}"))?;
        Ok(BVal::Int(val))
    }

    fn parse_bytes(&mut self) -> Result<BVal, String> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b':' {
                break;
            }
            if !b.is_ascii_digit() {
                return Err(format!(
                    "Invalid character in byte-string length at {}",
                    self.pos
                ));
            }
            self.consume();
        }
        let sep = self
            .consume()
            .ok_or_else(|| "Unterminated byte-string length".to_string())?;
        if sep != b':' {
            return Err("Expected ':' after byte-string length".to_string());
        }
        let len_str = std::str::from_utf8(&self.data[start..self.pos - 1])
            .map_err(|e| format!("Invalid utf-8 in string length: {e}"))?;
        let len = len_str
            .parse::<usize>()
            .map_err(|e| format!("Invalid byte-string length '{len_str}': {e}"))?;

        if self.pos + len > self.data.len() {
            return Err(format!(
                "Byte string of length {} exceeds buffer size (pos={}, total={})",
                len,
                self.pos,
                self.data.len()
            ));
        }

        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(BVal::Bytes(slice.to_vec()))
    }

    fn parse_list(&mut self) -> Result<BVal, String> {
        self.consume(); // 'l'
        self.depth += 1;
        let mut list = Vec::new();
        loop {
            match self.peek() {
                Some(b'e') => {
                    self.consume();
                    break;
                }
                Some(_) => {
                    let v = self.parse_val()?;
                    list.push(v);
                }
                None => {
                    self.depth -= 1;
                    return Err("Unterminated list in bencode".to_string());
                }
            }
        }
        self.depth -= 1;
        Ok(BVal::List(list))
    }

    fn parse_dict(&mut self) -> Result<BVal, String> {
        self.consume(); // 'd'
        self.depth += 1;
        let mut map = BTreeMap::new();
        loop {
            match self.peek() {
                Some(b'e') => {
                    self.consume();
                    break;
                }
                Some(b'0'..=b'9') => {
                    let key_val = self.parse_bytes()?;
                    let key = match key_val {
                        BVal::Bytes(b) => b,
                        _ => unreachable!(),
                    };
                    let val = self.parse_val()?;
                    map.insert(key, val);
                }
                Some(other) => {
                    self.depth -= 1;
                    return Err(format!(
                        "Invalid dict key token '{:02x}' at offset {}",
                        other, self.pos
                    ));
                }
                None => {
                    self.depth -= 1;
                    return Err("Unterminated dict in bencode".to_string());
                }
            }
        }
        self.depth -= 1;
        Ok(BVal::Dict(map))
    }
}

pub fn parse_bencode(data: &[u8]) -> Result<BVal, String> {
    BencodeParser::new(data).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_values() {
        assert_eq!(parse_bencode(b"i42e").unwrap(), BVal::Int(42));
        assert_eq!(parse_bencode(b"i-100e").unwrap(), BVal::Int(-100));
        assert_eq!(
            parse_bencode(b"4:spam").unwrap(),
            BVal::Bytes(b"spam".to_vec())
        );
        assert_eq!(
            parse_bencode(b"l4:spami42ee").unwrap(),
            BVal::List(vec![BVal::Bytes(b"spam".to_vec()), BVal::Int(42)])
        );
    }

    #[test]
    fn parses_dict_with_non_utf8_bytes() {
        let mut input = Vec::new();
        input.extend_from_slice(b"d4:path");
        let non_utf8 = vec![0xff, 0xfe, 0x00, 0x80];
        input.extend_from_slice(format!("{}:", non_utf8.len()).as_bytes());
        input.extend_from_slice(&non_utf8);
        input.extend_from_slice(b"e");

        let parsed = parse_bencode(&input).unwrap();
        let bytes = parsed.dict_get_bytes("path").unwrap();
        assert_eq!(bytes, non_utf8.as_slice());
        // Lossy UTF-8 conversion does not fail
        let str_lossy = parsed.dict_get_str("path").unwrap();
        assert!(!str_lossy.is_empty());
    }

    #[test]
    fn protects_against_recursion() {
        let mut nested = Vec::new();
        for _ in 0..150 {
            nested.extend_from_slice(b"l");
        }
        for _ in 0..150 {
            nested.extend_from_slice(b"e");
        }
        assert!(parse_bencode(&nested).is_err());
    }
}
