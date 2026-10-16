import { useEffect, useState } from 'react';
import { useParams, Link as RouterLink } from 'react-router-dom';
import { motion } from 'framer-motion';
import { MapPin, Globe, AlertTriangle, ArrowUpRight } from 'lucide-react';
import { API_ENDPOINTS } from '../config/api';
import SEO from '../components/SEO';
import { isSafeHttpUrl } from '../utils';
import { bioThemeId, type BioThemeId } from './bioThemes';

/** Colour classes for one bio theme. `''` is the page as it shipped: do not
 *  restyle it when adding a theme. */
const BIO_LOOK: Record<BioThemeId, {
    panel: string;
    title: string;
    bio: string;
    meta: string;
    website: string;
    link: string;
    icon: string;
    fallback: string;
    avatar: string;
    empty: string;
    footer: string;
}> = {
    '': {
        panel: '',
        title: 'text-2xl font-bold text-slate-900',
        bio: 'mt-2 text-slate-600',
        meta: 'mt-3 flex items-center justify-center gap-4 text-sm text-slate-500',
        website: 'inline-flex items-center gap-1 text-primary-600 hover:text-primary-700',
        link: 'group flex items-center justify-between gap-3 rounded-xl border border-slate-200 bg-white px-5 py-4 font-medium text-slate-900 shadow-sm transition-all hover:border-primary-300 hover:shadow-md',
        icon: 'h-4 w-4 flex-shrink-0 text-slate-400 transition-colors group-hover:text-primary-600',
        fallback: 'h-24 w-24 rounded-full bg-primary-100 text-primary-700 flex items-center justify-center mx-auto mb-4 text-3xl font-bold',
        avatar: 'h-24 w-24 rounded-full object-cover mx-auto mb-4 border border-slate-200',
        empty: 'text-center text-slate-500 text-sm',
        footer: 'text-xs text-slate-500 hover:text-slate-700',
    },
    ink: {
        panel: 'rounded-3xl bg-ink px-6 py-10',
        title: 'text-2xl font-bold text-paper',
        bio: 'mt-2 text-primary-100',
        meta: 'mt-3 flex items-center justify-center gap-4 text-sm text-primary-200',
        website: 'inline-flex items-center gap-1 text-primary-200 hover:text-paper',
        link: 'group flex items-center justify-between gap-3 rounded-xl border border-white/15 bg-white/10 px-5 py-4 font-medium text-paper shadow-sm transition-all hover:border-primary-300 hover:bg-white/15',
        icon: 'h-4 w-4 flex-shrink-0 text-primary-200 transition-colors group-hover:text-paper',
        fallback: 'h-24 w-24 rounded-full bg-primary-800 text-paper flex items-center justify-center mx-auto mb-4 text-3xl font-bold',
        avatar: 'h-24 w-24 rounded-full object-cover mx-auto mb-4 border border-white/20',
        empty: 'text-center text-primary-200 text-sm',
        footer: 'text-xs text-primary-200 hover:text-paper',
    },
    cobalt: {
        panel: 'rounded-3xl bg-primary-950 px-6 py-10',
        title: 'text-2xl font-bold text-primary-50',
        bio: 'mt-2 text-primary-100',
        meta: 'mt-3 flex items-center justify-center gap-4 text-sm text-primary-200',
        website: 'inline-flex items-center gap-1 text-primary-200 hover:text-primary-50',
        link: 'group flex items-center justify-between gap-3 rounded-xl border border-primary-700 bg-primary-900 px-5 py-4 font-medium text-primary-50 shadow-sm transition-all hover:border-primary-300',
        icon: 'h-4 w-4 flex-shrink-0 text-primary-300 transition-colors group-hover:text-primary-50',
        fallback: 'h-24 w-24 rounded-full bg-primary-800 text-primary-50 flex items-center justify-center mx-auto mb-4 text-3xl font-bold',
        avatar: 'h-24 w-24 rounded-full object-cover mx-auto mb-4 border border-primary-700',
        empty: 'text-center text-primary-200 text-sm',
        footer: 'text-xs text-primary-200 hover:text-primary-50',
    },
};

interface BioLink {
    code: string;
    short_url: string;
    label: string;
}

interface BioProfile {
    username: string;
    display_name?: string | null;
    bio?: string | null;
    website?: string | null;
    avatar_url?: string | null;
    location?: string | null;
    theme?: string | null;
    links: BioLink[];
}

export default function Bio() {
    const params = useParams<{ username?: string; code?: string }>();
    const raw = params.username ?? params.code ?? '';
    const username = raw.startsWith('@') ? raw.slice(1) : raw;
    const [profile, setProfile] = useState<BioProfile | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(false);
    const [avatarFailed, setAvatarFailed] = useState(false);

    useEffect(() => {
        let cancelled = false;
        (async () => {
            try {
                const res = await fetch(API_ENDPOINTS.bioPublic(username || ''));
                if (cancelled) return;
                if (res.ok) {
                    setProfile(await res.json());
                } else {
                    setError(true);
                }
            } catch {
                if (!cancelled) setError(true);
            } finally {
                if (!cancelled) setLoading(false);
            }
        })();
        return () => { cancelled = true; };
    }, [username]);

    if (loading) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center">
                <div className="h-8 w-8 border-4 border-primary-200 border-t-primary-600 rounded-full animate-spin" />
            </div>
        );
    }

    if (error || !profile) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center px-4">
                <SEO title="Profile not found" description="This profile could not be found." noIndex={true} />
                <div className="text-center">
                    <AlertTriangle className="h-16 w-16 text-amber-500 mx-auto mb-4" />
                    <h1 className="text-2xl font-bold text-slate-900 mb-2">Profile not found</h1>
                    <p className="text-slate-600 mb-6">This profile doesn't exist or isn't public.</p>
                    <RouterLink
                        to="/"
                        className="inline-flex items-center gap-2 bg-primary-600 text-white px-6 py-3 rounded-lg font-medium hover:bg-primary-700"
                    >
                        Go Home
                    </RouterLink>
                </div>
            </div>
        );
    }

    const name = profile.display_name || `@${profile.username}`;
    const initial = name.replace('@', '').charAt(0).toUpperCase() || '?';
    const theme = bioThemeId(profile.theme);
    const look = BIO_LOOK[theme];

    return (
        <div className="min-h-[60vh] px-4 py-12">
            {/* Bio pages are not indexed by default — they're public only to those
                who have the link. */}
            <SEO
                title={name}
                description={profile.bio || `${name} on opn.onl`}
                url={`/@${profile.username}`}
                noIndex={true}
            />

            <motion.div
                initial={{ opacity: 0, y: 16 }}
                animate={{ opacity: 1, y: 0 }}
                data-bio-theme={theme || 'light'}
                className={`max-w-md mx-auto ${look.panel}`}
            >
                {/* Header */}
                <div className="text-center mb-8">
                    {isSafeHttpUrl(profile.avatar_url) && !avatarFailed ? (
                        <img
                            src={API_ENDPOINTS.bioAvatar(profile.avatar_url!)}
                            alt={name}
                            className={look.avatar}
                            onError={() => setAvatarFailed(true)}
                        />
                    ) : (
                        <div className={look.fallback}>
                            {initial}
                        </div>
                    )}
                    <h1 className={look.title}>{name}</h1>
                    {profile.bio && <p className={look.bio}>{profile.bio}</p>}
                    <div className={look.meta}>
                        {profile.location && (
                            <span className="inline-flex items-center gap-1">
                                <MapPin className="h-4 w-4" /> {profile.location}
                            </span>
                        )}
                        {isSafeHttpUrl(profile.website) && (
                            <a
                                href={profile.website!}
                                target="_blank"
                                rel="noopener noreferrer"
                                className={look.website}
                            >
                                <Globe className="h-4 w-4" /> Website
                            </a>
                        )}
                    </div>
                </div>

                {/* Links */}
                <div className="space-y-3">
                    {profile.links.length === 0 ? (
                        <p className={look.empty}>No links yet.</p>
                    ) : (
                        profile.links.map((l) => (
                            <a
                                key={l.code}
                                href={l.short_url}
                                rel="noopener"
                                className={look.link}
                            >
                                <span className="truncate">{l.label}</span>
                                <ArrowUpRight className={look.icon} />
                            </a>
                        ))
                    )}
                </div>

                {/* Footer */}
                <div className="mt-10 text-center">
                    <RouterLink to="/" className={look.footer}>
                        Powered by opn.onl
                    </RouterLink>
                </div>
            </motion.div>
        </div>
    );
}
