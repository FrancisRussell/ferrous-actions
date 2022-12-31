use crate::actions::core;
use crate::Error;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

const JOB_INPUT: &str = "internal-use-github-job";
const MATRIX_INPUT: &str = "internal-use-matrix";
const WORKFLOW_INPUT: &str = "internal-use-github-workflow";

#[derive(Clone, Debug)]
pub struct Job {
    workflow: String,
    job_id: String,
    matrix_properties: BTreeMap<String, String>,
}

impl Job {
    fn get_json_input<T>(name: &str) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let input = core::Input::from(name).get_required()?;
        Ok(serde_json::from_str(&input)?)
    }

    pub fn from_env() -> Result<Job, Error> {
        let workflow = Self::get_json_input(WORKFLOW_INPUT)?;
        let job_id = Self::get_json_input(JOB_INPUT)?;
        let matrix_properties: Option<_> = Self::get_json_input(MATRIX_INPUT)?;
        let matrix_properties = matrix_properties.unwrap_or_default();
        let result = Job {
            workflow,
            job_id,
            matrix_properties,
        };
        Ok(result)
    }
}
