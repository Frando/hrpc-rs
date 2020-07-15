use parking_lot::Mutex;
use std::clone::Clone;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub struct Sessions<T> {
    inner: Arc<Mutex<InnerSessions<T>>>,
}

impl<T> Clone for Sessions<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Sessions<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerSessions::new())),
        }
    }
    pub fn insert(&self, item: T) -> u64 {
        self.inner.lock().insert(item)
    }
    pub fn take(&self, id: &u64) -> Option<T> {
        self.inner.lock().take(id)
    }
}

struct InnerSessions<T> {
    map: HashMap<u64, T>,
    free: VecDeque<u64>,
}

impl<T> InnerSessions<T> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            free: VecDeque::new(),
        }
    }

    pub fn insert(&mut self, item: T) -> u64 {
        let id = if let Some(id) = self.free.pop_front() {
            id
        } else {
            self.map.len() as u64 + 1
        };
        self.map.insert(id, item);
        id
    }

    pub fn take(&mut self, id: &u64) -> Option<T> {
        if let Some(item) = self.map.remove(id) {
            self.free.push_back(*id);
            Some(item)
        } else {
            None
        }
    }

    // pub fn has(&self, id: &u64) -> bool {
    //     self.map.get(id).is_some()
    // }
}
