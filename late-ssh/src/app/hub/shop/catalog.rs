use late_core::models::marketplace::{CAT_COMPANION_SKU, CHAT_BADGE_SLOT};

use super::svc::ShopCatalogItem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShopCategory {
    Companions,
    Badges,
}

impl ShopCategory {
    pub const ALL: [Self; 2] = [Self::Companions, Self::Badges];

    pub fn label(self) -> &'static str {
        match self {
            Self::Companions => "Companions",
            Self::Badges => "Badges",
        }
    }

    pub fn matches_item(self, item: &ShopCatalogItem) -> bool {
        match self {
            Self::Companions => item.item_kind == "feature_unlock",
            Self::Badges => item.is_chat_badge(),
        }
    }
}

pub fn is_cat_companion_sku(sku: &str) -> bool {
    sku == CAT_COMPANION_SKU
}

pub fn is_chat_badge_slot(slot: Option<&str>) -> bool {
    slot == Some(CHAT_BADGE_SLOT)
}
