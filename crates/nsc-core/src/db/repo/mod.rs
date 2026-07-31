pub mod novel;
pub mod chapter;
pub mod transformation;
pub mod prompt;
pub mod model_config;
pub mod data_asset;

pub use novel::{TransformationNovelRepo, UploadRepo};
pub use chapter::ChapterRepo;
pub use transformation::TransformationChapterRepo;
pub use prompt::PromptRepo;
pub use model_config::ModelConfigRepo;
pub use data_asset::DataAssetRepo;