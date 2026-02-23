# Pseudocode: categories (C4)

## File: `crates/unimatrix-server/src/categories.rs`

### Types

```
struct CategoryAllowlist:
    categories: RwLock<HashSet<String>>
```

### Initial Categories

```
const INITIAL_CATEGORIES: [&str; 6] = [
    "outcome",
    "lesson-learned",
    "decision",
    "convention",
    "pattern",
    "procedure",
]
```

### Implementation

```
impl CategoryAllowlist:
    fn new() -> Self:
        let mut set = HashSet::new()
        for cat in INITIAL_CATEGORIES:
            set.insert(cat.to_string())
        CategoryAllowlist { categories: RwLock::new(set) }

    fn validate(&self, category: &str) -> Result<(), ServerError>:
        let cats = self.categories.read().unwrap()  // RwLock read
        if cats.contains(category):
            Ok(())
        else:
            let mut valid: Vec<String> = cats.iter().cloned().collect()
            valid.sort()
            Err(ServerError::InvalidCategory {
                category: category.to_string(),
                valid_categories: valid,
            })

    fn add_category(&self, category: String):
        let mut cats = self.categories.write().unwrap()  // RwLock write
        cats.insert(category)

    fn list_categories(&self) -> Vec<String>:
        let cats = self.categories.read().unwrap()
        let mut list: Vec<String> = cats.iter().cloned().collect()
        list.sort()
        list
```

### Key Constraints
- Case-sensitive validation (all initial categories are lowercase)
- RwLock allows concurrent reads (validation) with rare writes (extension)
- Error message includes sorted list of valid categories
- Not backed by redb -- runtime-only
- Empty string category will fail validation (not in the set)
