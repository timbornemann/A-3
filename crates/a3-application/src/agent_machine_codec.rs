use crate::AgentActionDecodeError;
use a3_domain::{
    AgentAction, AgentMachineAction, ContentHash, MachineFileAction, MachineFileOperation,
    MachineFilePath, MachineHttpAction, MachineHttpUrl, MachineProcessAction, PatchFileContent,
    ProcessArgument, ProcessExecutable, TaskStepId,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u16,
    action: Selection,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "kind")]
enum Selection {
    #[serde(rename = "machine")]
    Machine { tool: Tool },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
enum Tool {
    File {
        version: u16,
        step_id: String,
        path: String,
        operation: FileOperation,
    },
    Process {
        version: u16,
        step_id: String,
        program: String,
        args: Vec<String>,
    },
    HttpGet {
        version: u16,
        step_id: String,
        url: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
enum FileOperation {
    Read {
        expected_hash: Value,
    },
    Write {
        expected_hash: Value,
        content: String,
    },
    Delete {
        expected_hash: String,
    },
}

pub(crate) fn decode_machine_document(raw: &str) -> Result<AgentAction, AgentActionDecodeError> {
    // Typed deserialization also rejects duplicate fields in this new privileged contract.
    let document: Envelope =
        serde_json::from_str(raw).map_err(|_| AgentActionDecodeError::InvalidShape)?;
    if document.schema_version != 6 {
        return Err(AgentActionDecodeError::UnsupportedVersion);
    }
    let Selection::Machine { tool } = document.action;
    let invalid = |_| AgentActionDecodeError::InvalidValue;
    let machine = match tool {
        Tool::File {
            version,
            step_id,
            path,
            operation,
        } => {
            check_version(version)?;
            let op = match operation {
                FileOperation::Read { expected_hash } => {
                    MachineFileOperation::Read(optional_hash(&expected_hash)?)
                }
                FileOperation::Write {
                    expected_hash,
                    content,
                } => MachineFileOperation::Write {
                    expected: optional_hash(&expected_hash)?,
                    content: PatchFileContent::try_from_bytes(content.into_bytes())
                        .map_err(invalid)?,
                },
                FileOperation::Delete { expected_hash } => MachineFileOperation::Delete(
                    ContentHash::from_bytes(crate::agent_action_codec::hex_id(&expected_hash)?),
                ),
            };
            AgentMachineAction::File(
                MachineFileAction::new(
                    step(&step_id)?,
                    MachineFilePath::new(path).map_err(|_| AgentActionDecodeError::InvalidValue)?,
                    op,
                )
                .map_err(|_| AgentActionDecodeError::InvalidValue)?,
            )
        }
        Tool::Process {
            version,
            step_id,
            program,
            args,
        } => {
            check_version(version)?;
            let program = ProcessExecutable::try_from_string(program)
                .map_err(|_| AgentActionDecodeError::InvalidValue)?;
            let args = args
                .into_iter()
                .map(|a| {
                    ProcessArgument::try_from_string(a)
                        .map_err(|_| AgentActionDecodeError::InvalidValue)
                })
                .collect::<Result<_, _>>()?;
            AgentMachineAction::Process(
                MachineProcessAction::new(step(&step_id)?, program, args)
                    .map_err(|_| AgentActionDecodeError::InvalidValue)?,
            )
        }
        Tool::HttpGet {
            version,
            step_id,
            url,
        } => {
            check_version(version)?;
            AgentMachineAction::HttpGet(MachineHttpAction::new(
                step(&step_id)?,
                MachineHttpUrl::new(url).map_err(|_| AgentActionDecodeError::InvalidValue)?,
            ))
        }
    };
    Ok(AgentAction::Machine(machine))
}
fn check_version(version: u16) -> Result<(), AgentActionDecodeError> {
    if version == 1 {
        Ok(())
    } else {
        Err(AgentActionDecodeError::UnsupportedVersion)
    }
}
fn step(value: &str) -> Result<TaskStepId, AgentActionDecodeError> {
    Ok(TaskStepId::from_bytes(crate::agent_action_codec::hex_id(
        value,
    )?))
}
fn optional_hash(value: &Value) -> Result<Option<ContentHash>, AgentActionDecodeError> {
    if value.is_null() {
        return Ok(None);
    }
    let value = value.as_str().ok_or(AgentActionDecodeError::InvalidShape)?;
    Ok(Some(ContentHash::from_bytes(
        crate::agent_action_codec::hex_id(value)?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn machine_v1_is_closed_bounded_and_never_backported() -> Result<(), Box<dyn std::error::Error>>
    {
        let step = "a".repeat(64);
        for tool in [
            json!({"kind":"file","version":1,"step_id":step,"path":"D:/fixture.txt","operation":{"kind":"read","expected_hash":null}}),
            json!({"kind":"file","version":1,"step_id":step,"path":"/fixture.txt","operation":{"kind":"write","expected_hash":null,"content":"hello"}}),
            json!({"kind":"process","version":1,"step_id":step,"program":"python","args":["--version"]}),
            json!({"kind":"http_get","version":1,"step_id":step,"url":"https://example.com/"}),
        ] {
            let mut document = json!({"schema_version":6,"action":{"kind":"machine","tool":tool}});
            assert!(matches!(
                crate::DecodeAgentAction::current().decode(&document.to_string())?,
                AgentAction::Machine(_)
            ));
            document["action"]["tool"]["risk"] = json!("safe");
            assert!(
                crate::DecodeAgentAction::current()
                    .decode(&document.to_string())
                    .is_err()
            );
            document["action"]["tool"]
                .as_object_mut()
                .ok_or("tool")?
                .remove("risk");
            document["schema_version"] = json!(5);
            assert!(
                crate::DecodeAgentAction::current()
                    .decode(&document.to_string())
                    .is_err()
            );
        }
        let raw = format!(
            r#"{{"schema_version":6,"action":{{"kind":"machine","tool":{{"kind":"process","version":1,"step_id":"{step}","program":"python","program":"sh","args":[]}}}}}}"#
        );
        assert!(crate::DecodeAgentAction::current().decode(&raw).is_err());
        for operation in [
            json!({"kind":"delete","expected_hash":null}),
            json!({"kind":"write","expected_hash":null,"content":"é".repeat(32769)}),
        ] {
            let document = json!({"schema_version":6,"action":{"kind":"machine","tool":{"kind":"file","version":1,"step_id":step,"path":"/fixture.txt","operation":operation}}});
            assert!(
                crate::DecodeAgentAction::current()
                    .decode(&document.to_string())
                    .is_err()
            );
        }
        let document = json!({"schema_version":6,"action":{"kind":"machine","tool":{"kind":"process","version":1,"step_id":step,"program":"python","args":vec!["x";33]}}});
        assert!(
            crate::DecodeAgentAction::current()
                .decode(&document.to_string())
                .is_err()
        );
        Ok(())
    }
}
