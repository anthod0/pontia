use pontia_core::{Error, Result};

pub fn encode(agent: &str, session: &str) -> Result<String> {
    if agent.is_empty() || session.is_empty() {
        return Err(Error::Domain(
            "ACP identity components must not be empty".into(),
        ));
    }
    Ok(serde_json::to_string(&[agent, session])?)
}

pub fn decode(key: &str) -> Result<[String; 2]> {
    let identity: [String; 2] = serde_json::from_str(key)?;
    if encode(&identity[0], &identity[1])? != key {
        return Err(Error::Domain("Noncanonical ACP session identity".into()));
    }
    Ok(identity)
}
