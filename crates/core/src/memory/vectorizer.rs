use crate::CoreError;

/// Embedding vectorizer trait
pub trait Vectorizer: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, CoreError>;
}

/// ONNX Runtime-based vectorizer using nomic-embed-text-v1 (stub for Phase 2 V1)
pub struct OnnxVectorizer;

impl OnnxVectorizer {
    pub fn new(model_path: &str) -> Result<Self, CoreError> {
        if !std::path::Path::new(model_path).exists() {
            return Err(CoreError::Memory(format!("ONNX model not found: {model_path}")));
        }
        Ok(Self)
    }
}

impl Vectorizer for OnnxVectorizer {
    fn embed(&self, _text: &str) -> Result<Vec<f32>, CoreError> {
        // Stub: return 768-dim zero vector
        // Real implementation loads ort::Session and runs inference
        Ok(vec![0.0f32; 768])
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, CoreError> {
        Ok(texts.iter().map(|_| vec![0.0f32; 768]).collect())
    }
}

/// Mock vectorizer for testing — hash-based deterministic embeddings
pub struct MockVectorizer;

impl Vectorizer for MockVectorizer {
    fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
        let mut vec = vec![0.0f32; 768];
        let hash: u64 = text.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        let idx = (hash % 768) as usize;
        vec[idx] = 1.0;
        Ok(vec)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, CoreError> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}
