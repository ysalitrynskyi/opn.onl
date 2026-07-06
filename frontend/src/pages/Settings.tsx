import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import {
    Key, Shield, Download, Trash2,
    ChevronRight, Loader2, Check, AlertTriangle,
    Fingerprint, Plus, User, Edit2, X, Globe, MapPin
} from 'lucide-react';
import { API_ENDPOINTS, authFetch } from '../config/api';
import SEO from '../components/SEO';
import logger from '../utils/logger';

interface Passkey {
    id: number;
    name: string;
    created_at: string;
    last_used: string | null;
}

interface UserProfile {
    id: number;
    email: string;
    email_verified: boolean;
    is_admin: boolean;
    created_at: string;
    link_count: number;
    total_clicks: number;
    display_name: string | null;
    bio: string | null;
    website: string | null;
    avatar_url: string | null;
    location: string | null;
    bio_username: string | null;
    bio_enabled: boolean;
    bio_theme: string | null;
}

interface AppSettings {
    account_deletion_enabled: boolean;
    custom_aliases_enabled: boolean;
    max_links_per_user: number | null;
    passkeys_enabled: boolean;
    link_in_bio_enabled: boolean;
    api_keys_enabled: boolean;
}

function errorMessage(err: unknown, fallback = 'Something went wrong'): string {
    return err instanceof Error ? err.message : fallback;
}

export default function Settings() {
    const navigate = useNavigate();
    const [loading, setLoading] = useState(true);
    const [profile, setProfile] = useState<UserProfile | null>(null);
    const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
    const [passkeys, setPasskeys] = useState<Passkey[]>([]);
    const [registeringPasskey, setRegisteringPasskey] = useState(false);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');

    // Change password state
    const [showChangePassword, setShowChangePassword] = useState(false);
    const [currentPassword, setCurrentPassword] = useState('');
    const [newPassword, setNewPassword] = useState('');
    const [confirmPassword, setConfirmPassword] = useState('');
    const [changingPassword, setChangingPassword] = useState(false);

    // Delete account state
    const [showDeleteAccount, setShowDeleteAccount] = useState(false);
    const [deletePassword, setDeletePassword] = useState('');
    const [deletingAccount, setDeletingAccount] = useState(false);

    // Resend verification state
    const [resendingVerification, setResendingVerification] = useState(false);

    // Rename passkey state
    const [renamingPasskeyId, setRenamingPasskeyId] = useState<number | null>(null);
    const [newPasskeyName, setNewPasskeyName] = useState('');

    // Profile editing state
    const [editingProfile, setEditingProfile] = useState(false);
    const [displayName, setDisplayName] = useState('');
    const [bio, setBio] = useState('');
    const [website, setWebsite] = useState('');
    const [location, setLocation] = useState('');
    const [savingProfile, setSavingProfile] = useState(false);

    // Link-in-bio state
    const [bioUsername, setBioUsername] = useState('');
    const [bioEnabled, setBioEnabled] = useState(false);
    const [savingBio, setSavingBio] = useState(false);

    // API keys state
    const [apiKeys, setApiKeys] = useState<{ id: number; name: string; key_prefix: string; last_used_at: string | null; created_at: string }[]>([]);
    const [newKeyName, setNewKeyName] = useState('');
    const [createdApiKey, setCreatedApiKey] = useState<string | null>(null);
    const [creatingKey, setCreatingKey] = useState(false);

    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        fetchData();
    }, [navigate]);

    // Sync link-in-bio form with the loaded profile.
    useEffect(() => {
        if (profile) {
            setBioUsername(profile.bio_username || '');
            setBioEnabled(profile.bio_enabled ?? false);
        }
    }, [profile]);

    const fetchData = async () => {
        try {
            setLoading(true);
            const [profileRes, settingsRes, passkeysRes, apiKeysRes] = await Promise.all([
                authFetch(API_ENDPOINTS.userProfile),
                fetch(API_ENDPOINTS.appSettings),
                authFetch(API_ENDPOINTS.passkeys),
                authFetch(API_ENDPOINTS.apiKeys),
            ]);

            if (profileRes.ok) {
                setProfile(await profileRes.json());
            }
            if (settingsRes.ok) {
                setAppSettings(await settingsRes.json());
            }
            if (passkeysRes.ok) {
                const data = await passkeysRes.json();
                setPasskeys(data.passkeys || []);
            }
            if (apiKeysRes.ok) {
                setApiKeys(await apiKeysRes.json());
            }
        } catch (err) {
            logger.error('Failed to fetch settings data', err);
        } finally {
            setLoading(false);
        }
    };

    const handleRegisterPasskey = async () => {
        setRegisteringPasskey(true);
        setError('');
        setSuccess('');

        try {
            const token = localStorage.getItem('token');
            if (!token) return;

            const payload = JSON.parse(atob(token.split('.')[1]));
            const username = payload.sub;

            // Step 1: Start registration. Uses authFetch so the Bearer token is
            // sent — the backend binds the new passkey to the authenticated
            // account, so enrollment must be authenticated.
            const startRes = await authFetch(API_ENDPOINTS.passkeyRegisterStart, {
                method: 'POST',
                body: JSON.stringify({ username }),
            });

            if (!startRes.ok) {
                throw new Error('Failed to start passkey registration');
            }

            const { options } = await startRes.json();

            // Step 2: Create credential with browser API
            const credential = await navigator.credentials.create({
                publicKey: {
                    ...options.publicKey,
                    challenge: base64ToBuffer(options.publicKey.challenge),
                    user: {
                        ...options.publicKey.user,
                        id: base64ToBuffer(options.publicKey.user.id),
                    },
                    excludeCredentials: options.publicKey.excludeCredentials?.map((cred: { id: string }) => ({
                        ...cred,
                        id: base64ToBuffer(cred.id),
                    })),
                },
            }) as PublicKeyCredential;

            if (!credential) {
                throw new Error('No credential returned');
            }

            const attestation = credential.response as AuthenticatorAttestationResponse;

            // Step 3: Finish registration (authenticated — see Step 1).
            const finishRes = await authFetch(API_ENDPOINTS.passkeyRegisterFinish, {
                method: 'POST',
                body: JSON.stringify({
                    username,
                    credential: {
                        id: credential.id,
                        rawId: bufferToBase64(credential.rawId),
                        response: {
                            attestationObject: bufferToBase64(attestation.attestationObject),
                            clientDataJSON: bufferToBase64(attestation.clientDataJSON),
                        },
                        type: credential.type,
                    },
                }),
            });

            if (!finishRes.ok) {
                throw new Error('Failed to finish passkey registration');
            }

            setSuccess('Passkey registered successfully!');
            fetchData(); // Refresh passkeys list
        } catch (err) {
            setError(errorMessage(err, 'Failed to register passkey'));
        } finally {
            setRegisteringPasskey(false);
        }
    };

    const handleDeletePasskey = async (passkeyId: number) => {
        if (!confirm('Are you sure you want to delete this passkey?')) return;

        try {
            const res = await authFetch(API_ENDPOINTS.passkeyDelete, {
                method: 'POST',
                body: JSON.stringify({ passkey_id: passkeyId }),
            });

            if (!res.ok) {
                const data = await res.json();
                throw new Error(data.error || 'Failed to delete passkey');
            }

            setSuccess('Passkey deleted successfully');
            fetchData();
        } catch (err) {
            setError(errorMessage(err));
        }
    };

    const handleRenamePasskey = async (passkeyId: number) => {
        if (!newPasskeyName.trim()) return;

        try {
            const res = await authFetch(API_ENDPOINTS.passkeyRename, {
                method: 'POST',
                body: JSON.stringify({ passkey_id: passkeyId, name: newPasskeyName }),
            });

            if (!res.ok) throw new Error('Failed to rename passkey');

            setRenamingPasskeyId(null);
            setNewPasskeyName('');
            fetchData();
        } catch (err) {
            setError(errorMessage(err));
        }
    };

    const handleChangePassword = async (e: React.FormEvent) => {
        e.preventDefault();
        if (newPassword !== confirmPassword) {
            setError('Passwords do not match');
            return;
        }
        if (newPassword.length < 8) {
            setError('Password must be at least 8 characters');
            return;
        }

        setChangingPassword(true);
        setError('');

        try {
            const res = await authFetch(API_ENDPOINTS.changePassword, {
                method: 'POST',
                body: JSON.stringify({
                    current_password: currentPassword,
                    new_password: newPassword,
                }),
            });

            if (!res.ok) {
                const data = await res.json();
                throw new Error(data.error || 'Failed to change password');
            }

            setSuccess('Password changed successfully');
            setShowChangePassword(false);
            setCurrentPassword('');
            setNewPassword('');
            setConfirmPassword('');
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setChangingPassword(false);
        }
    };

    const handleDeleteAccount = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!confirm('Are you SURE you want to delete your account? This cannot be undone!')) return;

        setDeletingAccount(true);
        setError('');

        try {
            const res = await authFetch(API_ENDPOINTS.deleteAccount, {
                method: 'POST',
                body: JSON.stringify({
                    email: profile?.email,
                    password: deletePassword,
                }),
            });

            if (!res.ok) {
                const data = await res.json();
                throw new Error(data.error || 'Failed to delete account');
            }

            localStorage.removeItem('token');
            navigate('/');
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setDeletingAccount(false);
        }
    };

    const handleResendVerification = async () => {
        setResendingVerification(true);
        setError('');

        try {
            const res = await fetch(API_ENDPOINTS.resendVerification, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ email: profile?.email }),
            });

            if (!res.ok) throw new Error('Failed to resend verification email');
            setSuccess('Verification email sent!');
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setResendingVerification(false);
        }
    };

    const handleExportData = async () => {
        try {
            const response = await authFetch(API_ENDPOINTS.exportLinks);

            if (!response.ok) throw new Error('Failed to export data');

            const blob = await response.blob();
            const url = window.URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = 'opn_onl_links.csv';
            document.body.appendChild(a);
            a.click();
            a.remove();
            window.URL.revokeObjectURL(url);
        } catch (err) {
            setError(errorMessage(err));
        }
    };

    const handleEditProfile = () => {
        setDisplayName(profile?.display_name || '');
        setBio(profile?.bio || '');
        setWebsite(profile?.website || '');
        setLocation(profile?.location || '');
        setEditingProfile(true);
    };

    const handleSaveProfile = async (e: React.FormEvent) => {
        e.preventDefault();
        setSavingProfile(true);
        setError('');

        try {
            const res = await authFetch(API_ENDPOINTS.updateProfile, {
                method: 'PUT',
                body: JSON.stringify({
                    display_name: displayName || undefined,
                    bio: bio || undefined,
                    website: website || undefined,
                    location: location || undefined,
                }),
            });

            if (!res.ok) {
                const data = await res.json();
                throw new Error(data.error || 'Failed to update profile');
            }

            const data = await res.json();
            setProfile(data);
            setEditingProfile(false);
            setSuccess('Profile updated successfully');
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setSavingProfile(false);
        }
    };

    const handleSaveBio = async (e: React.FormEvent) => {
        e.preventDefault();
        setSavingBio(true);
        setError('');
        setSuccess('');
        try {
            const res = await authFetch(API_ENDPOINTS.bioSettings, {
                method: 'PUT',
                body: JSON.stringify({
                    bio_username: bioUsername || '',
                    bio_enabled: bioEnabled,
                }),
            });
            if (!res.ok) {
                const txt = await res.text();
                throw new Error(txt || 'Failed to save bio settings');
            }
            const data = await res.json();
            setBioUsername(data.bio_username || '');
            setBioEnabled(data.bio_enabled ?? false);
            setSuccess('Bio page settings saved');
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setSavingBio(false);
        }
    };

    const handleCreateApiKey = async (e: React.FormEvent) => {
        e.preventDefault();
        setCreatingKey(true);
        setError('');
        setSuccess('');
        setCreatedApiKey(null);
        try {
            const res = await authFetch(API_ENDPOINTS.apiKeys, {
                method: 'POST',
                body: JSON.stringify({ name: newKeyName || undefined }),
            });
            if (!res.ok) {
                const txt = await res.text();
                throw new Error(txt || 'Failed to create API key');
            }
            const data = await res.json();
            setCreatedApiKey(data.key);
            setNewKeyName('');
            setSuccess("API key created — copy it now, it won't be shown again.");
            await fetchData();
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setCreatingKey(false);
        }
    };

    const handleRevokeApiKey = async (id: number) => {
        setError('');
        setSuccess('');
        try {
            const res = await authFetch(API_ENDPOINTS.apiKey(id), { method: 'DELETE' });
            if (!res.ok) {
                const txt = await res.text();
                throw new Error(txt || 'Failed to revoke API key');
            }
            setApiKeys(prev => prev.filter(k => k.id !== id));
            setSuccess('API key revoked');
        } catch (err) {
            setError(errorMessage(err));
        }
    };

    if (loading) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center">
                <Loader2 className="h-8 w-8 animate-spin text-primary-600" />
            </div>
        );
    }

    const inputClass = "w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint";
    const labelClass = "block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5";

    return (
        <div className="max-w-3xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
            <SEO title="Settings" description="Manage your account settings" noIndex />

            <h1 className="font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight mb-8">Settings</h1>

            {error && (
                <motion.div
                    initial={{ opacity: 0, y: -10 }}
                    animate={{ opacity: 1, y: 0 }}
                    role="alert"
                    className="mb-6 flex items-center gap-2 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-sm text-danger"
                >
                    <AlertTriangle className="h-4 w-4 flex-shrink-0" />
                    {error}
                    <button onClick={() => setError('')} aria-label="Dismiss error" className="ml-auto text-danger/70 hover:text-danger">
                        <X className="h-4 w-4" />
                    </button>
                </motion.div>
            )}

            {success && (
                <motion.div
                    initial={{ opacity: 0, y: -10 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="mb-6 flex items-center gap-2 rounded-xl border border-success/30 bg-success/5 px-4 py-3 text-sm text-success"
                >
                    <Check className="h-4 w-4 flex-shrink-0" />
                    {success}
                    <button onClick={() => setSuccess('')} aria-label="Dismiss" className="ml-auto text-success/70 hover:text-success">
                        <X className="h-4 w-4" />
                    </button>
                </motion.div>
            )}

            <div className="space-y-6">
                {/* Profile Section */}
                <motion.section
                    initial={{ opacity: 0, y: 16 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
                >
                    <div className="p-6 border-b border-line">
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <div className="flex h-10 w-10 items-center justify-center rounded-full border border-line bg-paper">
                                    <User className="h-5 w-5 text-muted" />
                                </div>
                                <div>
                                    <h2 className="font-display text-lg font-bold text-ink tracking-tight">
                                        {profile?.display_name || 'Profile'}
                                    </h2>
                                    <p className="text-sm text-muted">{profile?.email}</p>
                                </div>
                            </div>
                            <div className="flex items-center gap-2">
                                {profile?.email_verified ? (
                                    <span className="inline-flex items-center gap-1 rounded-full border border-success/30 bg-success/5 px-2.5 py-1 text-xs font-medium text-success">
                                        <Check className="h-3 w-3" />
                                        Verified
                                    </span>
                                ) : (
                                    <span className="inline-flex items-center gap-1 rounded-full border border-warning/40 bg-warning/10 px-2.5 py-1 text-xs font-medium text-warning">
                                        <AlertTriangle className="h-3 w-3" />
                                        Unverified
                                    </span>
                                )}
                                {!editingProfile && (
                                    <button
                                        onClick={handleEditProfile}
                                        className="p-2 text-faint transition-colors hover:text-ink"
                                        title="Edit profile"
                                        aria-label="Edit profile"
                                    >
                                        <Edit2 className="h-4 w-4" />
                                    </button>
                                )}
                            </div>
                        </div>
                    </div>
                    <div className="p-6 space-y-4">
                        {editingProfile ? (
                            <form onSubmit={handleSaveProfile} className="space-y-4">
                                <div>
                                    <label htmlFor="display-name" className={labelClass}>Display Name</label>
                                    <input
                                        id="display-name"
                                        type="text"
                                        value={displayName}
                                        onChange={(e) => setDisplayName(e.target.value)}
                                        className={inputClass}
                                        placeholder="Your name"
                                        maxLength={100}
                                    />
                                </div>
                                <div>
                                    <label htmlFor="bio" className={labelClass}>Bio</label>
                                    <textarea
                                        id="bio"
                                        value={bio}
                                        onChange={(e) => setBio(e.target.value)}
                                        className={`${inputClass} resize-none`}
                                        placeholder="Tell us about yourself"
                                        rows={3}
                                        maxLength={500}
                                    />
                                    <p className="text-xs text-faint mt-1">{bio.length}/500 characters</p>
                                </div>
                                <div className="grid grid-cols-2 gap-4">
                                    <div>
                                        <label htmlFor="website" className={labelClass}>Website</label>
                                        <input
                                            id="website"
                                            type="url"
                                            value={website}
                                            onChange={(e) => setWebsite(e.target.value)}
                                            className={inputClass}
                                            placeholder="https://example.com"
                                        />
                                    </div>
                                    <div>
                                        <label htmlFor="location" className={labelClass}>Location</label>
                                        <input
                                            id="location"
                                            type="text"
                                            value={location}
                                            onChange={(e) => setLocation(e.target.value)}
                                            className={inputClass}
                                            placeholder="City, Country"
                                            maxLength={100}
                                        />
                                    </div>
                                </div>
                                <div className="flex gap-3 pt-2">
                                    <button
                                        type="button"
                                        onClick={() => setEditingProfile(false)}
                                        className="rounded-lg border border-line2 px-4 py-2 font-medium text-muted transition-colors hover:text-ink hover:border-ink/30"
                                    >
                                        Cancel
                                    </button>
                                    <button
                                        type="submit"
                                        disabled={savingProfile}
                                        className="rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                                    >
                                        {savingProfile ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Save Profile'}
                                    </button>
                                </div>
                            </form>
                        ) : (
                            <>
                                {/* Profile info display */}
                                {(profile?.bio || profile?.website || profile?.location) && (
                                    <div className="space-y-2 pb-4 border-b border-line">
                                        {profile?.bio && (
                                            <p className="text-muted leading-relaxed">{profile.bio}</p>
                                        )}
                                        {(profile?.website || profile?.location) && (
                                            <div className="flex flex-wrap gap-4 text-sm text-muted">
                                                {profile?.website && (
                                                    <a href={profile.website} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 transition-colors hover:text-primary-600">
                                                        <Globe className="h-3.5 w-3.5 text-faint" /> {profile.website.replace(/^https?:\/\//, '')}
                                                    </a>
                                                )}
                                                {profile?.location && (
                                                    <span className="inline-flex items-center gap-1.5">
                                                        <MapPin className="h-3.5 w-3.5 text-faint" /> {profile.location}
                                                    </span>
                                                )}
                                            </div>
                                        )}
                                    </div>
                                )}

                                <div className="grid grid-cols-2 gap-px bg-line border border-line rounded-xl overflow-hidden">
                                    <div className="bg-surface p-4">
                                        <p className="font-mono text-xs uppercase tracking-[0.14em] text-faint">Total Links</p>
                                        <p className="mt-1 font-display text-2xl font-extrabold text-ink tabular-nums">{profile?.link_count?.toLocaleString() || 0}</p>
                                    </div>
                                    <div className="bg-surface p-4">
                                        <p className="font-mono text-xs uppercase tracking-[0.14em] text-faint">Total Clicks</p>
                                        <p className="mt-1 font-display text-2xl font-extrabold text-ink tabular-nums">{profile?.total_clicks?.toLocaleString() || 0}</p>
                                    </div>
                                </div>

                                {/* Show resend verification only if NOT verified */}
                                {profile && !profile.email_verified && (
                                    <div className="rounded-xl border border-warning/40 bg-warning/10 p-4">
                                        <div className="flex items-center justify-between gap-4">
                                            <div>
                                                <p className="font-medium text-ink">Email not verified</p>
                                                <p className="text-sm text-muted">Please verify your email to access all features.</p>
                                            </div>
                                            <button
                                                onClick={handleResendVerification}
                                                disabled={resendingVerification}
                                                className="shrink-0 rounded-lg border border-line2 bg-surface px-4 py-2 text-sm font-medium text-ink transition-colors hover:border-ink/30 disabled:opacity-50"
                                            >
                                                {resendingVerification ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Resend'}
                                            </button>
                                        </div>
                                    </div>
                                )}
                            </>
                        )}
                    </div>
                </motion.section>

                {/* Passkeys Section */}
                {appSettings?.passkeys_enabled && (
                    <motion.section
                        initial={{ opacity: 0, y: 16 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.05 }}
                        className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
                    >
                        <div className="p-6 border-b border-line">
                            <div className="flex items-center justify-between">
                                <div className="flex items-center gap-3">
                                    <div className="flex h-10 w-10 items-center justify-center rounded-full border border-line bg-paper">
                                        <Fingerprint className="h-5 w-5 text-muted" />
                                    </div>
                                    <div>
                                        <h2 className="font-display text-lg font-bold text-ink tracking-tight">Passkeys</h2>
                                        <p className="text-sm text-muted">Passwordless authentication</p>
                                    </div>
                                </div>
                                <button
                                    onClick={handleRegisterPasskey}
                                    disabled={registeringPasskey}
                                    className="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                                >
                                    {registeringPasskey ? (
                                        <Loader2 className="h-4 w-4 animate-spin" />
                                    ) : (
                                        <Plus className="h-4 w-4" />
                                    )}
                                    Add Passkey
                                </button>
                            </div>
                        </div>
                        <div className="divide-y divide-line">
                            {passkeys.length === 0 ? (
                                <div className="p-8 text-center text-muted">
                                    <Fingerprint className="mx-auto mb-3 h-10 w-10 text-line2" />
                                    <p className="text-ink">No passkeys registered</p>
                                    <p className="text-sm text-faint">Add a passkey for passwordless login</p>
                                </div>
                            ) : (
                                passkeys.map((passkey) => (
                                    <div key={passkey.id} className="flex items-center justify-between p-4">
                                        <div className="flex items-center gap-3">
                                            <Key className="h-5 w-5 text-faint" />
                                            <div>
                                                {renamingPasskeyId === passkey.id ? (
                                                    <div className="flex items-center gap-2">
                                                        <input
                                                            type="text"
                                                            value={newPasskeyName}
                                                            onChange={(e) => setNewPasskeyName(e.target.value)}
                                                            className="rounded border border-line2 bg-surface px-2 py-1 text-sm text-ink outline-none focus:border-primary-500"
                                                            placeholder="Passkey name"
                                                            aria-label="Passkey name"
                                                            autoFocus
                                                        />
                                                        <button
                                                            onClick={() => handleRenamePasskey(passkey.id)}
                                                            aria-label="Confirm rename"
                                                            className="text-success hover:opacity-80"
                                                        >
                                                            <Check className="h-4 w-4" />
                                                        </button>
                                                        <button
                                                            onClick={() => { setRenamingPasskeyId(null); setNewPasskeyName(''); }}
                                                            aria-label="Cancel rename"
                                                            className="text-faint hover:text-ink"
                                                        >
                                                            <X className="h-4 w-4" />
                                                        </button>
                                                    </div>
                                                ) : (
                                                    <p className="font-medium text-ink">{passkey.name}</p>
                                                )}
                                                <p className="text-xs text-faint">
                                                    Created {new Date(passkey.created_at).toLocaleDateString()}
                                                    {passkey.last_used && ` · Last used ${new Date(passkey.last_used).toLocaleDateString()}`}
                                                </p>
                                            </div>
                                        </div>
                                        <div className="flex items-center gap-1">
                                            <button
                                                onClick={() => { setRenamingPasskeyId(passkey.id); setNewPasskeyName(passkey.name); }}
                                                className="p-2 text-faint transition-colors hover:text-ink"
                                                title="Rename"
                                                aria-label="Rename passkey"
                                            >
                                                <Edit2 className="h-4 w-4" />
                                            </button>
                                            <button
                                                onClick={() => handleDeletePasskey(passkey.id)}
                                                className="p-2 text-faint transition-colors hover:text-danger"
                                                title="Delete"
                                                aria-label="Delete passkey"
                                            >
                                                <Trash2 className="h-4 w-4" />
                                            </button>
                                        </div>
                                    </div>
                                ))
                            )}
                        </div>
                    </motion.section>
                )}

                {/* Security Section */}
                <motion.section
                    initial={{ opacity: 0, y: 16 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.1 }}
                    className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
                >
                    <div className="p-6 border-b border-line">
                        <div className="flex items-center gap-3">
                            <div className="flex h-10 w-10 items-center justify-center rounded-full border border-line bg-paper">
                                <Shield className="h-5 w-5 text-muted" />
                            </div>
                            <div>
                                <h2 className="font-display text-lg font-bold text-ink tracking-tight">Security</h2>
                                <p className="text-sm text-muted">Manage your password</p>
                            </div>
                        </div>
                    </div>
                    <div className="p-6">
                        {!showChangePassword ? (
                            <button
                                onClick={() => setShowChangePassword(true)}
                                className="flex w-full items-center justify-between rounded-xl border border-line bg-paper p-4 transition-colors hover:border-line2 hover:bg-primary-50/40"
                            >
                                <div className="flex items-center gap-3">
                                    <Key className="h-5 w-5 text-faint" />
                                    <span className="font-medium text-ink">Change Password</span>
                                </div>
                                <ChevronRight className="h-5 w-5 text-faint" />
                            </button>
                        ) : (
                            <form onSubmit={handleChangePassword} className="space-y-4">
                                <input
                                    type="password"
                                    placeholder="Current password"
                                    aria-label="Current password"
                                    value={currentPassword}
                                    onChange={(e) => setCurrentPassword(e.target.value)}
                                    className={inputClass}
                                    required
                                />
                                <input
                                    type="password"
                                    placeholder="New password (min 8 characters)"
                                    aria-label="New password"
                                    value={newPassword}
                                    onChange={(e) => setNewPassword(e.target.value)}
                                    className={inputClass}
                                    required
                                    minLength={8}
                                />
                                <input
                                    type="password"
                                    placeholder="Confirm new password"
                                    aria-label="Confirm new password"
                                    value={confirmPassword}
                                    onChange={(e) => setConfirmPassword(e.target.value)}
                                    className={inputClass}
                                    required
                                />
                                <div className="flex gap-3">
                                    <button
                                        type="button"
                                        onClick={() => setShowChangePassword(false)}
                                        className="rounded-lg border border-line2 px-4 py-2 font-medium text-muted transition-colors hover:text-ink hover:border-ink/30"
                                    >
                                        Cancel
                                    </button>
                                    <button
                                        type="submit"
                                        disabled={changingPassword}
                                        className="rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                                    >
                                        {changingPassword ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Update Password'}
                                    </button>
                                </div>
                            </form>
                        )}
                    </div>
                </motion.section>

                {/* Data Section */}
                <motion.section
                    initial={{ opacity: 0, y: 16 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.15 }}
                    className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
                >
                    <div className="p-6 border-b border-line">
                        <div className="flex items-center gap-3">
                            <div className="flex h-10 w-10 items-center justify-center rounded-full border border-line bg-paper">
                                <Download className="h-5 w-5 text-muted" />
                            </div>
                            <div>
                                <h2 className="font-display text-lg font-bold text-ink tracking-tight">Data</h2>
                                <p className="text-sm text-muted">Export your data</p>
                            </div>
                        </div>
                    </div>
                    <div className="p-6">
                        <button
                            onClick={handleExportData}
                            className="flex w-full items-center justify-between rounded-xl border border-line bg-paper p-4 transition-colors hover:border-line2 hover:bg-primary-50/40"
                        >
                            <div className="flex items-center gap-3">
                                <Download className="h-5 w-5 text-faint" />
                                <span className="font-medium text-ink">Export All Links (CSV)</span>
                            </div>
                            <ChevronRight className="h-5 w-5 text-faint" />
                        </button>
                    </div>
                </motion.section>

                {/* API Keys — personal tokens for the MCP server / API clients */}
                {appSettings?.api_keys_enabled && (
                    <motion.section
                        initial={{ opacity: 0, y: 16 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.17 }}
                        className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
                    >
                        <div className="p-6 border-b border-line">
                            <div className="flex items-center gap-3">
                                <div className="flex h-10 w-10 items-center justify-center rounded-full border border-line bg-paper">
                                    <Key className="h-5 w-5 text-muted" />
                                </div>
                                <div>
                                    <h2 className="font-display text-lg font-bold text-ink tracking-tight">API Keys</h2>
                                    <p className="text-sm text-muted">Personal tokens for the MCP server and the API</p>
                                </div>
                            </div>
                        </div>
                        <div className="p-6 space-y-4">
                            {createdApiKey && (
                                <div className="rounded-lg border border-primary-200 bg-primary-50/60 p-4">
                                    <p className="text-sm font-medium text-ink mb-2">Your new API key — copy it now, it won't be shown again:</p>
                                    <div className="flex items-center gap-2">
                                        <code className="flex-1 break-all rounded-md border border-line2 bg-surface px-3 py-2 font-mono text-xs text-ink">{createdApiKey}</code>
                                        <button
                                            type="button"
                                            onClick={() => { navigator.clipboard?.writeText(createdApiKey); setSuccess('Copied to clipboard'); }}
                                            className="rounded-lg border border-line2 px-3 py-2 text-sm text-muted transition-colors hover:text-ink hover:border-ink/30"
                                        >
                                            Copy
                                        </button>
                                    </div>
                                </div>
                            )}

                            {apiKeys.length > 0 && (
                                <ul className="divide-y divide-line rounded-lg border border-line">
                                    {apiKeys.map(k => (
                                        <li key={k.id} className="flex items-center justify-between gap-3 px-4 py-3">
                                            <div className="min-w-0">
                                                <p className="font-medium text-ink truncate">{k.name}</p>
                                                <p className="font-mono text-xs text-faint">
                                                    {k.key_prefix}… · {k.last_used_at ? `last used ${new Date(k.last_used_at).toLocaleDateString()}` : 'never used'}
                                                </p>
                                            </div>
                                            <button
                                                type="button"
                                                onClick={() => handleRevokeApiKey(k.id)}
                                                className="inline-flex items-center gap-1 text-sm text-danger hover:underline"
                                                aria-label={`Revoke ${k.name}`}
                                            >
                                                <Trash2 className="h-4 w-4" /> Revoke
                                            </button>
                                        </li>
                                    ))}
                                </ul>
                            )}

                            <form onSubmit={handleCreateApiKey} className="flex items-center gap-2">
                                <input
                                    type="text"
                                    value={newKeyName}
                                    onChange={e => setNewKeyName(e.target.value)}
                                    placeholder="Key name (e.g. MCP on my laptop)"
                                    maxLength={60}
                                    className={inputClass}
                                />
                                <button
                                    type="submit"
                                    disabled={creatingKey}
                                    className="inline-flex items-center gap-1.5 whitespace-nowrap rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                                >
                                    {creatingKey ? <Loader2 className="h-4 w-4 animate-spin" /> : <><Plus className="h-4 w-4" /> Create key</>}
                                </button>
                            </form>
                            <p className="text-xs text-faint">
                                Use with the{' '}
                                <a href="https://github.com/ysalitrynskyi/opn-mcp" target="_blank" rel="noreferrer" className="text-primary-600 hover:underline">opn.onl MCP server</a>{' '}
                                or any API client: <code className="font-mono">Authorization: Bearer opn_…</code>
                            </p>
                        </div>
                    </motion.section>
                )}

                {/* Public profile (Link-in-Bio) — only shown when the instance enables it */}
                {appSettings?.link_in_bio_enabled && (
                    <motion.section
                        initial={{ opacity: 0, y: 16 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.18 }}
                        className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
                    >
                        <div className="p-6 border-b border-line">
                            <div className="flex items-center gap-3">
                                <div className="flex h-10 w-10 items-center justify-center rounded-full border border-line bg-paper">
                                    <User className="h-5 w-5 text-muted" />
                                </div>
                                <div>
                                    <h2 className="font-display text-lg font-bold text-ink tracking-tight">Public profile</h2>
                                    <p className="text-sm text-muted">A link-in-bio page for your links — off until you turn it on</p>
                                </div>
                            </div>
                        </div>
                        <form onSubmit={handleSaveBio} className="p-6 space-y-4">
                            <div>
                                <label htmlFor="bio-username" className="block text-sm font-medium text-ink mb-1.5">Username</label>
                                <div className="flex items-center gap-2">
                                    <span className="text-sm text-faint">opn.onl/@</span>
                                    <input
                                        id="bio-username"
                                        type="text"
                                        value={bioUsername}
                                        onChange={(e) => setBioUsername(e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, ''))}
                                        placeholder="yourname"
                                        maxLength={30}
                                        className={inputClass}
                                    />
                                </div>
                            </div>
                            <label className="flex items-center gap-2.5 text-sm text-ink cursor-pointer">
                                <input
                                    type="checkbox"
                                    checked={bioEnabled}
                                    onChange={(e) => setBioEnabled(e.target.checked)}
                                    className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
                                />
                                Make my bio page public
                            </label>
                            <p className="text-xs text-faint">
                                Choose which links appear from each link's edit dialog ("Show this link on my bio page").
                            </p>
                            <div className="flex items-center gap-3">
                                <button
                                    type="submit"
                                    disabled={savingBio}
                                    className="rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                                >
                                    {savingBio ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Save'}
                                </button>
                                {bioEnabled && bioUsername && (
                                    <a href={`/@${bioUsername}`} target="_blank" rel="noreferrer" className="text-sm font-medium text-primary-600 hover:text-primary-700">
                                        View page →
                                    </a>
                                )}
                            </div>
                        </form>
                    </motion.section>
                )}

                {/* Danger Zone - Only show if account deletion is enabled */}
                {appSettings?.account_deletion_enabled && (
                    <motion.section
                        initial={{ opacity: 0, y: 16 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.2 }}
                        className="rounded-2xl border border-danger/30 bg-surface shadow-subtle overflow-hidden"
                    >
                        <div className="p-6 border-b border-danger/20 bg-danger/5">
                            <div className="flex items-center gap-3">
                                <div className="flex h-10 w-10 items-center justify-center rounded-full border border-danger/30 bg-danger/10">
                                    <Trash2 className="h-5 w-5 text-danger" />
                                </div>
                                <div>
                                    <h2 className="font-display text-lg font-bold text-danger tracking-tight">Danger Zone</h2>
                                    <p className="text-sm text-danger/80">Irreversible actions</p>
                                </div>
                            </div>
                        </div>
                        <div className="p-6">
                            {!showDeleteAccount ? (
                                <button
                                    onClick={() => setShowDeleteAccount(true)}
                                    className="flex w-full items-center justify-between rounded-xl border border-danger/30 bg-danger/5 p-4 transition-colors hover:bg-danger/10"
                                >
                                    <div className="flex items-center gap-3">
                                        <Trash2 className="h-5 w-5 text-danger" />
                                        <span className="font-medium text-danger">Delete Account</span>
                                    </div>
                                    <ChevronRight className="h-5 w-5 text-danger/60" />
                                </button>
                            ) : (
                                <form onSubmit={handleDeleteAccount} className="space-y-4">
                                    <div className="rounded-xl border border-danger/30 bg-danger/5 p-4">
                                        <p className="text-sm font-medium text-danger">This action cannot be undone!</p>
                                        <p className="text-sm text-danger/80">All your links and data will be permanently deleted.</p>
                                    </div>
                                    <input
                                        type="password"
                                        placeholder="Enter your password to confirm"
                                        aria-label="Confirm password"
                                        value={deletePassword}
                                        onChange={(e) => setDeletePassword(e.target.value)}
                                        className="w-full rounded-lg border border-danger/40 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-danger placeholder:text-faint"
                                        required
                                    />
                                    <div className="flex gap-3">
                                        <button
                                            type="button"
                                            onClick={() => setShowDeleteAccount(false)}
                                            className="rounded-lg border border-line2 px-4 py-2 font-medium text-muted transition-colors hover:text-ink hover:border-ink/30"
                                        >
                                            Cancel
                                        </button>
                                        <button
                                            type="submit"
                                            disabled={deletingAccount}
                                            className="rounded-lg bg-danger px-4 py-2 font-semibold text-white transition-opacity hover:opacity-90 disabled:opacity-50"
                                        >
                                            {deletingAccount ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Delete My Account'}
                                        </button>
                                    </div>
                                </form>
                            )}
                        </div>
                    </motion.section>
                )}
            </div>
        </div>
    );
}

// Helper functions for WebAuthn
function base64ToBuffer(base64: string): ArrayBuffer {
    const padding = '='.repeat((4 - (base64.length % 4)) % 4);
    const b64 = (base64 + padding).replace(/-/g, '+').replace(/_/g, '/');
    const rawData = window.atob(b64);
    const outputArray = new Uint8Array(rawData.length);
    for (let i = 0; i < rawData.length; ++i) {
        outputArray[i] = rawData.charCodeAt(i);
    }
    return outputArray.buffer;
}

function bufferToBase64(buffer: ArrayBuffer): string {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.byteLength; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    return window.btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}
