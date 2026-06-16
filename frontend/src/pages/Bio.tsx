import { useEffect, useState } from 'react';
import { useParams, Link as RouterLink } from 'react-router-dom';
import { motion } from 'framer-motion';
import { MapPin, Globe, AlertTriangle, ArrowUpRight } from 'lucide-react';
import { API_ENDPOINTS } from '../config/api';
import SEO from '../components/SEO';

interface BioLink {
    code: string;
    short_url: string;
    label: string;
    click_count: number;
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
    const { username } = useParams<{ username: string }>();
    const [profile, setProfile] = useState<BioProfile | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(false);

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
                className="max-w-md mx-auto"
            >
                {/* Header */}
                <div className="text-center mb-8">
                    {profile.avatar_url ? (
                        <img
                            src={profile.avatar_url}
                            alt={name}
                            className="h-24 w-24 rounded-full object-cover mx-auto mb-4 border border-slate-200"
                        />
                    ) : (
                        <div className="h-24 w-24 rounded-full bg-primary-100 text-primary-700 flex items-center justify-center mx-auto mb-4 text-3xl font-bold">
                            {initial}
                        </div>
                    )}
                    <h1 className="text-2xl font-bold text-slate-900">{name}</h1>
                    {profile.bio && <p className="mt-2 text-slate-600">{profile.bio}</p>}
                    <div className="mt-3 flex items-center justify-center gap-4 text-sm text-slate-500">
                        {profile.location && (
                            <span className="inline-flex items-center gap-1">
                                <MapPin className="h-4 w-4" /> {profile.location}
                            </span>
                        )}
                        {profile.website && (
                            <a
                                href={profile.website}
                                target="_blank"
                                rel="noreferrer"
                                className="inline-flex items-center gap-1 text-primary-600 hover:text-primary-700"
                            >
                                <Globe className="h-4 w-4" /> Website
                            </a>
                        )}
                    </div>
                </div>

                {/* Links */}
                <div className="space-y-3">
                    {profile.links.length === 0 ? (
                        <p className="text-center text-slate-500 text-sm">No links yet.</p>
                    ) : (
                        profile.links.map((l) => (
                            <a
                                key={l.code}
                                href={l.short_url}
                                rel="noopener"
                                className="group flex items-center justify-between gap-3 rounded-xl border border-slate-200 bg-white px-5 py-4 font-medium text-slate-900 shadow-sm transition-all hover:border-primary-300 hover:shadow-md"
                            >
                                <span className="truncate">{l.label}</span>
                                <ArrowUpRight className="h-4 w-4 flex-shrink-0 text-slate-400 transition-colors group-hover:text-primary-600" />
                            </a>
                        ))
                    )}
                </div>

                {/* Footer */}
                <div className="mt-10 text-center">
                    <RouterLink to="/" className="text-xs text-slate-400 hover:text-slate-600">
                        Powered by opn.onl
                    </RouterLink>
                </div>
            </motion.div>
        </div>
    );
}
