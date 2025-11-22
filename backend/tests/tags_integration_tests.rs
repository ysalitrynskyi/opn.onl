//! Comprehensive tag integration tests

/// Mock tag for testing
struct MockTag {
    id: i32,
    name: String,
    color: Option<String>,
    user_id: Option<i32>,
    org_id: Option<i32>,
}

impl MockTag {
    fn new(id: i32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            color: None,
            user_id: Some(1),
            org_id: None,
        }
    }

    fn with_color(mut self, color: &str) -> Self {
        self.color = Some(color.to_string());
        self
    }
}

// ============= Tag Creation Tests =============

mod creation_tests {
    use super::*;

    #[test]
    fn test_create_tag_basic() {
        let tag = MockTag::new(1, "important");
        
        assert_eq!(tag.id, 1);
        assert_eq!(tag.name, "important");
        assert!(tag.color.is_none());
    }

    #[test]
    fn test_create_tag_with_color() {
        let tag = MockTag::new(1, "urgent").with_color("#FF0000");
        
        assert_eq!(tag.color, Some("#FF0000".to_string()));
    }
}

// ============= Tag Name Validation Tests =============

mod name_validation {
    fn is_valid_tag_name(name: &str) -> bool {
        !name.is_empty() 
            && name.len() <= 50 
            && name.trim() == name
            && !name.contains(',') // No commas (for CSV export compatibility)
    }

    #[test]
    fn test_valid_tag_names() {
        assert!(is_valid_tag_name("important"));
        assert!(is_valid_tag_name("work"));
        assert!(is_valid_tag_name("personal-stuff"));
        assert!(is_valid_tag_name("2024"));
        assert!(is_valid_tag_name("日本語タグ"));
    }

    #[test]
    fn test_invalid_tag_names() {
        assert!(!is_valid_tag_name(""));
        assert!(!is_valid_tag_name(" space"));
        assert!(!is_valid_tag_name("space "));
        assert!(!is_valid_tag_name("tag,with,commas"));
        assert!(!is_valid_tag_name(&"x".repeat(51)));
    }
}

// ============= Link-Tag Association Tests =============

mod association_tests {
    struct LinkTag {
        link_id: i32,
        tag_id: i32,
    }

    fn get_tags_for_link(link_id: i32, associations: &[LinkTag]) -> Vec<i32> {
        associations
            .iter()
            .filter(|lt| lt.link_id == link_id)
            .map(|lt| lt.tag_id)
            .collect()
    }

    fn get_links_for_tag(tag_id: i32, associations: &[LinkTag]) -> Vec<i32> {
        associations
            .iter()
            .filter(|lt| lt.tag_id == tag_id)
            .map(|lt| lt.link_id)
            .collect()
    }

    fn has_tag(link_id: i32, tag_id: i32, associations: &[LinkTag]) -> bool {
        associations.iter().any(|lt| lt.link_id == link_id && lt.tag_id == tag_id)
    }

    #[test]
    fn test_link_has_multiple_tags() {
        let associations = vec![
            LinkTag { link_id: 1, tag_id: 1 },
            LinkTag { link_id: 1, tag_id: 2 },
            LinkTag { link_id: 1, tag_id: 3 },
            LinkTag { link_id: 2, tag_id: 1 },
        ];
        
        let tags = get_tags_for_link(1, &associations);
        assert_eq!(tags.len(), 3);
        assert!(tags.contains(&1));
        assert!(tags.contains(&2));
        assert!(tags.contains(&3));
    }

    #[test]
    fn test_tag_has_multiple_links() {
        let associations = vec![
            LinkTag { link_id: 1, tag_id: 1 },
            LinkTag { link_id: 2, tag_id: 1 },
            LinkTag { link_id: 3, tag_id: 1 },
            LinkTag { link_id: 4, tag_id: 2 },
        ];
        
        let links = get_links_for_tag(1, &associations);
        assert_eq!(links.len(), 3);
    }

    #[test]
    fn test_has_tag() {
        let associations = vec![
            LinkTag { link_id: 1, tag_id: 1 },
            LinkTag { link_id: 1, tag_id: 2 },
        ];
        
        assert!(has_tag(1, 1, &associations));
        assert!(has_tag(1, 2, &associations));
        assert!(!has_tag(1, 3, &associations));
        assert!(!has_tag(2, 1, &associations));
    }

    #[test]
    fn test_empty_associations() {
        let associations: Vec<LinkTag> = vec![];
        
        assert!(get_tags_for_link(1, &associations).is_empty());
        assert!(get_links_for_tag(1, &associations).is_empty());
    }
}

// ============= Tag Filtering Tests =============

mod filtering_tests {
    struct Link {
        id: i32,
        tags: Vec<i32>,
    }

    fn filter_by_tag<'a>(links: &'a [Link], tag_id: i32) -> Vec<&'a Link> {
        links.iter().filter(|l| l.tags.contains(&tag_id)).collect()
    }

    fn filter_by_all_tags<'a>(links: &'a [Link], tag_ids: &[i32]) -> Vec<&'a Link> {
        links.iter().filter(|l| tag_ids.iter().all(|t| l.tags.contains(t))).collect()
    }

    fn filter_by_any_tag<'a>(links: &'a [Link], tag_ids: &[i32]) -> Vec<&'a Link> {
        links.iter().filter(|l| tag_ids.iter().any(|t| l.tags.contains(t))).collect()
    }

    #[test]
    fn test_filter_by_single_tag() {
        let links = vec![
            Link { id: 1, tags: vec![1, 2] },
            Link { id: 2, tags: vec![2, 3] },
            Link { id: 3, tags: vec![1, 3] },
        ];
        
        let filtered = filter_by_tag(&links, 1);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|l| l.id == 1));
        assert!(filtered.iter().any(|l| l.id == 3));
    }

    #[test]
    fn test_filter_by_all_tags() {
        let links = vec![
            Link { id: 1, tags: vec![1, 2, 3] },
            Link { id: 2, tags: vec![1, 2] },
            Link { id: 3, tags: vec![2, 3] },
        ];
        
        let filtered = filter_by_all_tags(&links, &[1, 2]);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|l| l.id == 1));
        assert!(filtered.iter().any(|l| l.id == 2));
    }

    #[test]
    fn test_filter_by_any_tag() {
        let links = vec![
            Link { id: 1, tags: vec![1] },
            Link { id: 2, tags: vec![2] },
            Link { id: 3, tags: vec![3] },
        ];
        
        let filtered = filter_by_any_tag(&links, &[1, 2]);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|l| l.id == 1));
        assert!(filtered.iter().any(|l| l.id == 2));
    }

    #[test]
    fn test_no_matching_tags() {
        let links = vec![
            Link { id: 1, tags: vec![1, 2] },
            Link { id: 2, tags: vec![2, 3] },
        ];
        
        let filtered = filter_by_tag(&links, 99);
        assert!(filtered.is_empty());
    }
}

// ============= Tag Bulk Operations Tests =============

mod bulk_operations {
    fn add_tags_to_links(link_ids: &[i32], tag_ids: &[i32]) -> Vec<(i32, i32)> {
        let mut result = Vec::new();
        for &link_id in link_ids {
            for &tag_id in tag_ids {
                result.push((link_id, tag_id));
            }
        }
        result
    }

    fn remove_tags_from_links(existing: &[(i32, i32)], link_ids: &[i32], tag_ids: &[i32]) -> Vec<(i32, i32)> {
        existing
            .iter()
            .filter(|(l, t)| !(link_ids.contains(l) && tag_ids.contains(t)))
            .copied()
            .collect()
    }

    #[test]
    fn test_bulk_add_tags() {
        let associations = add_tags_to_links(&[1, 2, 3], &[10, 20]);
        
        assert_eq!(associations.len(), 6);
        assert!(associations.contains(&(1, 10)));
        assert!(associations.contains(&(1, 20)));
        assert!(associations.contains(&(2, 10)));
        assert!(associations.contains(&(3, 20)));
    }

    #[test]
    fn test_bulk_remove_tags() {
        let existing = vec![
            (1, 10), (1, 20),
            (2, 10), (2, 20),
            (3, 10), (3, 20),
        ];
        
        let remaining = remove_tags_from_links(&existing, &[1, 2], &[10]);
        
        assert_eq!(remaining.len(), 4);
        assert!(!remaining.contains(&(1, 10)));
        assert!(!remaining.contains(&(2, 10)));
        assert!(remaining.contains(&(1, 20)));
        assert!(remaining.contains(&(3, 10)));
    }
}

