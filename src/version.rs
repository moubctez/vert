use html5ever::{
    local_name,
    tendril::StrTendril,
    tokenizer::{
        BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
    },
    Attribute, QualName,
};
use std::{cmp::Ordering, fmt, str::FromStr};

#[derive(Debug)]
pub struct Version {
    v: Vec<i32>,
}

impl Version {
    #[must_use]
    pub fn new(v: Vec<i32>) -> Self {
        Self { v }
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.v == *other.v
    }
}

impl PartialEq<Vec<i32>> for Version {
    fn eq(&self, other: &Vec<i32>) -> bool {
        self.v == *other
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.v.partial_cmp(&other.v)
    }
}

impl PartialOrd<Vec<i32>> for Version {
    fn partial_cmp(&self, other: &Vec<i32>) -> Option<Ordering> {
        self.v.partial_cmp(other)
    }
}

impl FromStr for Version {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(index) = s.find(|c: char| c.is_ascii_digit()) {
            // if let Some(index) = s.find(|c: char| c == '-' || c == '_') {
            let v: Vec<i32> = s[index..]
                .split(&['.', '-'])
                .map_while(|d| i32::from_str(d).ok())
                .collect();
            if v.len() > 1 {
                Ok(Self { v })
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for digit in &self.v {
            if first {
                first = false;
            } else {
                write!(f, ".")?;
            }
            write!(f, "{digit}")?;
        }
        Ok(())
    }
}

struct VersionSink {
    version: Option<Version>,
}

impl VersionSink {
    pub fn new() -> Self {
        Self { version: None }
    }
}

impl TokenSink for VersionSink {
    type Handle = ();

    // string_cache::Atom<LocalNameStaticSet>
    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        if let Token::TagToken(Tag {
            kind: TagKind::StartTag,
            name: local_name!("a"),
            attrs,
            ..
        }) = token
        {
            for attr in &attrs {
                if let Attribute {
                    name:
                        QualName {
                            local: local_name!("href"),
                            ..
                        },
                    value,
                } = attr
                {
                    if let Ok(version) = Version::from_str(value.as_ref()) {
                        match &self.version {
                            None => self.version = Some(version),
                            Some(v) => {
                                if v < &version {
                                    self.version = Some(version);
                                }
                            }
                        }
                    }
                }
            }
        }
        TokenSinkResult::Continue
    }
}

#[must_use]
pub fn parse_html(html: &str) -> Option<Version> {
    let mut chunk = StrTendril::new();
    chunk.push_slice(html);
    let mut input = BufferQueue::new();
    input.push_back(chunk.try_reinterpret().unwrap());

    let mut tok = Tokenizer::new(VersionSink::new(), TokenizerOpts::default());
    let _ = tok.feed(&mut input);
    tok.end();

    tok.sink.version
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(Version::from_str("package.tar.gz").is_err());
        assert!(Version::from_str("SHA256").is_err());

        let v = vec![1, 2, 3];
        assert_eq!(Version::from_str("1.2.3").unwrap(), v);
        assert_eq!(Version::from_str("1-2-3").unwrap(), v);
        assert_eq!(Version::from_str("01.02.03").unwrap(), v);
        assert_eq!(Version::from_str("package-1.2.3").unwrap(), v);
        assert_eq!(Version::from_str("package-1.2-3").unwrap(), v);
        assert_eq!(Version::from_str("package-1.2.3.tar.gz").unwrap(), v);
        assert_eq!(Version::from_str("package-1.2.3.post1").unwrap(), v);
        // assert_eq!(Version::from_str("xyz3-1.2.3").unwrap(), v);
    }

    #[test]
    fn test_version_string() {
        let version = Version::new(vec![1, 2, 3]);
        assert_eq!(&version.to_string(), "1.2.3");
    }

    #[test]
    fn test_parse_html() {
        let html = r#"<html>
<head><title>Index of /dist/</title></head>
<body>
<h1>Index of /dist/</h1><hr><pre><a href="../">../</a>
<a href="README">README</a>                                             12-Jun-2022 20:55                7893
<a href="SHA256">SHA256</a>                                             12-Jun-2022 20:57               13753
<a href="SHA256.sig">SHA256.sig</a>                                         12-Jun-2022 20:58                 566
<a href="sudo-1.8.0.tar.gz">sudo-1.8.0.tar.gz</a>                                  25-Feb-2011 19:58             1209024
<a href="sudo-1.8.0.tar.gz.sig">sudo-1.8.0.tar.gz.sig</a>                              04-Dec-2017 22:45                 543
<a href="sudo-1.8.1.tar.gz">sudo-1.8.1.tar.gz</a>                                  09-Apr-2011 15:17             1238495
<a href="sudo-1.8.1.tar.gz.sig">sudo-1.8.1.tar.gz.sig</a>                              04-Dec-2017 22:45                 543
<a href="sudo-1.8.10.tar.gz">sudo-1.8.10.tar.gz</a>                                 10-Mar-2014 12:35             2259801
<a href="sudo-1.8.10.tar.gz.sig">sudo-1.8.10.tar.gz.sig</a>                             04-Dec-2017 22:45                 543
<a href="sudo-1.8.10p1.patch.gz">sudo-1.8.10p1.patch.gz</a>                             13-Mar-2014 21:22                4879
<a href="sudo-1.8.10p1.patch.gz.sig">sudo-1.8.10p1.patch.gz.sig</a>                         04-Dec-2017 22:45                 543
<a href="sudo-1.8.10p1.tar.gz">sudo-1.8.10p1.tar.gz</a>                               13-Mar-2014 21:20             2260994
<a href="sudo-1.8.10p1.tar.gz.sig">sudo-1.8.10p1.tar.gz.sig</a>                           04-Dec-2017 22:45                 543
<a href="sudo-1.8.10p2.patch.gz">sudo-1.8.10p2.patch.gz</a>                             17-Mar-2014 14:33                2692
</body></html>
"#;
        let v = parse_html(html);
        assert_eq!(v, Some(Version::new(vec![1, 8, 10])));
    }
}
