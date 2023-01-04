use crate::actions::core;
use crate::Error;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;

const JOB_INPUT: &str = "internal-use-github-job";
const MATRIX_INPUT: &str = "internal-use-matrix";
const WORKFLOW_INPUT: &str = "internal-use-github-workflow";

#[derive(Clone, Debug, Hash)]
pub struct Job {
    workflow: String,
    job_id: String,
    matrix_properties: Option<BTreeMap<String, String>>,
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
        let matrix_properties = Self::get_json_input(MATRIX_INPUT)?;
        let result = Job {
            workflow,
            job_id,
            matrix_properties,
        };
        Ok(result)
    }

    pub fn get_workflow(&self) -> &str {
        &self.workflow
    }

    pub fn get_job_id(&self) -> &str {
        &self.job_id
    }

    pub fn matrix_properties_as_string(&self) -> Option<String> {
        // Note: This function does not attempt to guarantee that this string is
        // deterministic. At the time of writing it is though, regardless of whether
        // serde_json's "preserve_order" feature is on or off.
        self.matrix_properties
            .as_ref()
            .map(|p| serde_json::to_string(p).expect("Failed to serialize a map of String to String to JSON"))
    }
}
