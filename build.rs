use std::env;

pub const DEFAULT_CONTRACT_FOLDER: &str = "../tests/osmosis.yaml";
pub const DEFAULT_ARTIFACTS_FOLDER: &str = "../artifacts";

use integration_tests_config::config::TestConfig;

fn main() {  
    let build_enabled = env::var("BUILD_ENABLED")
        .map(|v| v == "1")
        .unwrap_or(false);

    if build_enabled {
        println!("Building artifacts");
        let config: TestConfig = TestConfig::from_yaml(DEFAULT_CONTRACT_FOLDER);
        config.build(DEFAULT_ARTIFACTS_FOLDER).unwrap();
    }else{
        println!("Skip building artifacts");
    }
}