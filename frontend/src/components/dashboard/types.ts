export interface LinkData {
    id: number;
    code: string;
    original_url: string;
    short_url: string;
    api_url?: string;
    title: string | null;
    click_count: number;
    created_at: string;
    expires_at: string | null;
    has_password: boolean;
    notes: string | null;
    is_active: boolean;
    is_pinned: boolean;
    burn_after_reading?: boolean;
    burned_at?: string | null;
    safe_link_interstitial?: boolean;
    bio_visible?: boolean;
    tags: { id: number; name: string; color: string }[];
}

export interface RoutingRule {
    match_device?: string | null;
    match_os?: string | null;
    match_country?: string | null;
    match_lang?: string | null;
    destination_url: string;
    weight?: number;
    priority?: number;
}

export interface LinkUpdatePayload {
    original_url?: string;
    password?: string;
    expires_at?: string;
    remove_password?: boolean;
    remove_expiration?: boolean;
    burn_after_reading?: boolean;
    safe_link_interstitial?: boolean;
    bio_visible?: boolean;
}

export interface AppSettings {
    custom_aliases_enabled: boolean;
    min_alias_length: number;
    max_alias_length: number;
    qr_branding_enabled: boolean;
    burn_after_reading_enabled: boolean;
    safe_link_interstitial_enabled: boolean;
    conditional_routing_enabled: boolean;
    link_in_bio_enabled: boolean;
}

export type LinkSortBy = 'date' | 'clicks';
export type LinkSortOrder = 'asc' | 'desc';

export interface SparklineSeries {
    data: number[];
    labels: string[];
}
