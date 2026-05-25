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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_vectorizer_deterministic() {
        let v = MockVectorizer;
        let emb1 = v.embed("hello world").unwrap();
        let emb2 = v.embed("hello world").unwrap();
        assert_eq!(emb1, emb2, "Same input should produce same embedding");
    }

    #[test]
    fn test_mock_vectorizer_different_inputs() {
        let v = MockVectorizer;
        let emb1 = v.embed("hello").unwrap();
        let emb2 = v.embed("world").unwrap();
        assert_ne!(emb1, emb2, "Different inputs should produce different embeddings");
    }

    #[test]
    fn test_mock_vectorizer_dimension() {
        let v = MockVectorizer;
        let emb = v.embed("test").unwrap();
        assert_eq!(emb.len(), 768, "Embedding should be 768-dimensional");
    }

    #[test]
    fn test_mock_vectorizer_one_hot() {
        let v = MockVectorizer;
        let emb = v.embed("x").unwrap();
        let ones = emb.iter().filter(|&&x| x == 1.0).count();
        assert_eq!(ones, 1, "Should have exactly one 1.0 entry");
    }

    #[test]
    fn test_mock_vectorizer_batch() {
        let v = MockVectorizer;
        let embs = v.embed_batch(&["a", "b", "c"]).unwrap();
        assert_eq!(embs.len(), 3);
        assert_eq!(embs[0].len(), 768);
    }

    #[test]
    fn test_onnx_vectorizer_model_not_found() {
        let result = OnnxVectorizer::new("/nonexistent/model.onnx");
        assert!(result.is_err());
    }
}
