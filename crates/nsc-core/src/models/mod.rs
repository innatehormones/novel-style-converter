pub mod novel;
pub mod chapter;
pub mod transformation;
pub mod prompt;
pub mod model_config;
pub mod data_asset;
pub mod batch;

pub use novel::{NewTransformationNovel, NewUpload, TransformationNovel, Upload};
pub use chapter::{Chapter, NewChapter};
pub use transformation::{NewTransformationChapter, TransformationChapter, TransformMode, TransformStatus};
pub use prompt::{Prompt, PromptKind};
pub use model_config::{ModelConfig, NewModelConfig};
pub use data_asset::{DataAsset, NewDataAsset};
pub use batch::{Batch, BatchStatus, NewBatch, OnFailurePolicy, ResumeAction};
pub mod workflow_result;

pub use workflow_result::{WorkflowResult, WorkflowResultChapter};
