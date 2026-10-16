import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import {
    Key, Shield, Download, Trash2,
    ChevronRight, Loader2, Check, AlertTriangle,
    Fingerprint, Plus, User, Edit2, X, Globe, MapPin, Bell
} from 'lucide-react';
import { API_ENDPOINTS, authFetch, failureMessage, responseError } from '../config/api';
import SEO from '../components/SEO';
import BioSettingsSection from './BioSettingsSection';
import logger from '../utils/logger';
import { formatDate, isSafeHttpUrl } from '../utils';
import { safeLocalStorage } from '../utils/storage';

interface Passkey {
    id: number;
    name: string;
    created_at: string;
    last_used: string | null;
}

interface ApiKeyRow {
    id: number;
    name: string;
    key_prefix: string;
    last_used_at: string | null;
    created_at: string;
    scope: string;
    expires_at: string | null;
    expired: boolean;
}

type ApiKeyScopeChoice = 'full' | 'read_only';
type ApiKeyExpiryChoice = 'never' | '30' | '90' | '365';

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
    notify_link_expiry: boolean;
}

interface AppSettings {
    account_deletion_enabled: boolean;
    // Days until a deleted account's data is removed for good; null when the
    // operator keeps it until an admin removes it. Operator-configured, so the
    // page quotes it instead of hard-coding a number.
    account_purge_days?: number | null;
    custom_aliases_enabled: boolean;
    max_links_per_user: number | null;
    passkeys_enabled: boolean;
    link_in_bio_enabled: boolean;
    api_keys_enabled: boolean;
    // Absent on an older server. Missing means verification is still required.
    require_email_verification?: boolean;
    // True only when this process can send mail. Unknown hides the quiet resend.
    email_configured?: boolean;
    // Whether owners are mailed before their links expire (the instance switch
    // is on and mail is configured). Absent on an older server, which sends none.
    link_expiry_reminders_enabled?: boolean;
}

function errorMessage(err: unknown, fallback = 'Something went wrong'): string {
    return err instanceof Error ? err.message : fallback;
}

// What happens to an account's data after it is deleted, as the server reports it.
function retentionNotice(purgeDays: number | null | undefined): string {
    if (typeof purgeDays !== 'number') {
        return 'Your data stays on the server until an administrator removes it.';
    }
    return `Your email address, profile and links are permanently removed ${purgeDays} ${purgeDays === 1 ? 'day' : 'days'} after you delete the account. Until then only an administrator can restore it; after that nothing can.`;
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

    // Expiry reminder switch state
    const [savingReminders, setSavingReminders] = useState(false);

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

    // API keys state
    const [apiKeys, setApiKeys] = useState<ApiKeyRow[]>([]);
    const [newKeyName, setNewKeyName] = useState('');
    const [keyScope, setKeyScope] = useState<ApiKeyScopeChoice>('full');
    const [keyExpiry, setKeyExpiry] = useState<ApiKeyExpiryChoice>('never');
    const [createdApiKey, setCreatedApiKey] = useState<string | null>(null);
    const [creatingKey, setCreatingKey] = useState(false);
    const [copyFailed, setCopyFailed] = useState(false);
    const createdKeyRef = useRef<HTMLElement>(null);

    useEffect(() => {
        const token = safeLocalStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        fetchData();
    }, [navigate]);

    const fetchData = async () => {
        try {
            setLoading(true);
            const [profileRes, settingsRes, passkeysRes, apiKeysRes] = await Promise.all([
                authFetch(API_ENDPOINTS.userProfile),
                fetch(API_ENDPOINTS.appSettings),
                authFetch(API_ENDPOINTS.passkeys),
                authFetch(API_ENDPOINTS.apiKeys),
            ]);

            if (!profileRes.ok) {
                const data = await profileRes.json().catch(() => null) as { error?: string } | null;
                setError(data?.error || 'Failed to load account settings. Please try again.');
                setProfile(null);
                return;
            }

            setProfile(await profileRes.json());
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
            setError(failureMessage(err, 'Failed to load account settings. Please try again.'));
            setProfile(null);
        } finally {
            setLoading(false);
        }
    };

    const handleRegisterPasskey = async () => {
        setRegisteringPasskey(true);
        setError('');
        setSuccess('');

        try {
            // register_start binds the passkey to the authenticated account;
            // the username field is ignored server-side. Use the loaded profile
            // rather than decoding the JWT — atob rejects base64url payloads.
            const username = profile?.email ?? '';

            // Step 1: Start registration. Uses authFetch so the Bearer token is
            // sent — the backend binds the new passkey to the authenticated
            // account, so enrollment must be authenticated.
            const startRes = await authFetch(API_ENDPOINTS.passkeyRegisterStart, {
                method: 'POST',
                body: JSON.stringify({ username }),
            });

            if (!startRes.ok) {
                throw new Error(await responseError(startRes, 'Failed to start passkey registration'));
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
                throw new Error(await responseError(finishRes, 'Failed to finish passkey registration'));
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

            // Deleting a passkey revokes every older session, this one
            // included. Keep the replacement token, or the reload below is
            // rejected and the user is sent to /login.
            const data = await res.json().catch(() => null) as { token?: string } | null;
            if (data?.token) {
                safeLocalStorage.setItem('token', data.token);
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

            const data = await res.json();
            if (!res.ok) {
                throw new Error(data.error || 'Failed to change password');
            }

            // The backend rotates our JWT on password change (revoking the old
            // one). Store the fresh token so we stay logged in instead of 401-ing
            // on the next request and being bounced to /login.
            if (data.token) {
                safeLocalStorage.setItem('token', data.token);
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
        if (!confirm(`Delete your account? You are signed out and your links stop working right away. ${retentionNotice(appSettings?.account_purge_days)}`)) return;

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

            safeLocalStorage.removeItem('token');
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
            // Signed in, so the server answers for this account specifically:
            // sent, already verified, wait N minutes, or no email on this
            // server. Anonymous callers only ever get "if the account exists".
            const res = await authFetch(API_ENDPOINTS.resendVerification, {
                method: 'POST',
                body: JSON.stringify({ email: profile?.email }),
            });

            if (!res.ok) {
                throw new Error(await responseError(res, 'Failed to resend verification email'));
            }
            const body = await res.json().catch(() => null) as { message?: string } | null;
            setSuccess(body?.message || 'Verification email sent');
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setResendingVerification(false);
        }
    };

    const handleToggleExpiryReminders = async (on: boolean) => {
        setSavingReminders(true);
        setError('');
        setSuccess('');

        try {
            const res = await authFetch(API_ENDPOINTS.updateProfile, {
                method: 'PUT',
                body: JSON.stringify({ notify_link_expiry: on }),
            });
            if (!res.ok) {
                throw new Error(await responseError(res, 'Failed to save the reminder setting'));
            }
            setProfile(await res.json());
            setSuccess(on ? 'Expiry reminders turned on' : 'Expiry reminders turned off');
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setSavingReminders(false);
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
            if (website.trim() && !isSafeHttpUrl(website)) {
                throw new Error('Website must be an http or https URL');
            }
            // An empty string clears a field; a missing key leaves it as it is.
            const res = await authFetch(API_ENDPOINTS.updateProfile, {
                method: 'PUT',
                body: JSON.stringify({
                    display_name: displayName,
                    bio,
                    website,
                    location,
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

    const handleCreateApiKey = async (e: React.FormEvent) => {
        e.preventDefault();
        setCreatingKey(true);
        setError('');
        setSuccess('');
        try {
            const body: { name?: string; scope: ApiKeyScopeChoice; expires_in_days?: number } = {
                scope: keyScope,
            };
            if (newKeyName) body.name = newKeyName;
            if (keyExpiry !== 'never') body.expires_in_days = Number(keyExpiry);
            const res = await authFetch(API_ENDPOINTS.apiKeys, {
                method: 'POST',
                body: JSON.stringify(body),
            });
            if (!res.ok) {
                throw new Error(await responseError(res, 'Failed to create API key'));
            }
            const data = await res.json();
            setCreatedApiKey(data.key);
            setCopyFailed(false);
            setNewKeyName('');
            setKeyScope('full');
            setKeyExpiry('never');
            setSuccess("API key created — copy it now, it won't be shown again.");
            // Do not refetch: fetchData sets loading and can null profile,
            // which unmounts this banner. The plaintext secret is shown once.
            if (data.id != null) {
                setApiKeys(prev => [{
                    id: data.id,
                    name: data.name || 'API key',
                    key_prefix: data.key_prefix || String(data.key).slice(0, 12),
                    last_used_at: null,
                    created_at: data.created_at || new Date().toISOString(),
                    scope: data.scope === 'read_only' ? 'read_only' : 'full',
                    expires_at: data.expires_at ?? null,
                    expired: Boolean(data.expired),
                }, ...prev]);
            }
        } catch (err) {
            setError(errorMessage(err));
        } finally {
            setCreatingKey(false);
        }
    };

    const handleCopyApiKey = async () => {
        if (!createdApiKey) return;
        try {
            // Throws when there is no clipboard API (a page served over plain
            // HTTP has none) and when the browser refuses the write.
            await navigator.clipboard.writeText(createdApiKey);
            setCopyFailed(false);
            setSuccess('Copied to clipboard');
        } catch {
            // The key is shown once. Select it so Ctrl/Cmd+C still works, and
            // say it was not copied.
            const key = createdKeyRef.current;
            if (key) {
                const range = document.createRange();
                range.selectNodeContents(key);
                const selection = window.getSelection();
                selection?.removeAllRanges();
                selection?.addRange(range);
            }
            setCopyFailed(true);
        }
    };

    const handleRevokeApiKey = async (id: number) => {
        if (!confirm('Are you sure you want to revoke this API key? It cannot be recovered.')) return;

        setError('');
        setSuccess('');
        try {
            const res = await authFetch(API_ENDPOINTS.apiKey(id), { method: 'DELETE' });
            if (!res.ok) {
                throw new Error(await responseError(res, 'Failed to revoke API key'));
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

    if (!profile) {
        return (
            <div className="max-w-3xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
                <SEO title="Settings" description="Manage your account settings" noIndex />
                <h1 className="font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight mb-8">Settings</h1>
                <div
                    role="alert"
                    className="mb-6 flex flex-col sm:flex-row sm:items-center gap-3 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-sm text-danger"
                >
                    <div className="flex items-center gap-2 min-w-0">
                        <AlertTriangle className="h-4 w-4 flex-shrink-0" />
                        <span>{error || 'Failed to load account settings. Please try again.'}</span>
                    </div>
                    <button
                        type="button"
                        onClick={() => { setError(''); fetchData(); }}
                        className="sm:ml-auto shrink-0 rounded-lg border border-danger/30 bg-surface px-3 py-1.5 text-sm font-medium text-ink transition-colors hover:border-ink/30"
                    >
                        Try again
                    </button>
                </div>
            </div>
        );
    }

    const inputClass = "w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint";
    const labelClass = "block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5";
    // Default stays "required", including while settings are missing.
    const verificationRequired = appSettings?.require_email_verification !== false;
    const passkeyNeedsVerification = !profile.email_verified && verificationRequired;
    // The server skips unverified owners while verification is required.
    const remindersNeedVerification = !profile.email_verified && verificationRequired;
    // The nag and its button stay while verification is required, so a server
    // that cannot send mail can say so. Once it is optional, offer a resend
    // only when mail is actually configured.
    const showResend = !profile.email_verified && (verificationRequired || appSettings?.email_configured === true);

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
                                            maxLength={2048}
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
                                {(profile?.bio || isSafeHttpUrl(profile?.website) || profile?.location) && (
                                    <div className="space-y-2 pb-4 border-b border-line">
                                        {profile?.bio && (
                                            <p className="text-muted leading-relaxed">{profile.bio}</p>
                                        )}
                                        {(isSafeHttpUrl(profile?.website) || profile?.location) && (
                                            <div className="flex flex-wrap gap-4 text-sm text-muted">
                                                {isSafeHttpUrl(profile?.website) && (
                                                    <a href={profile!.website!} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1.5 transition-colors hover:text-primary-600">
                                                        <Globe className="h-3.5 w-3.5 text-faint" /> {profile!.website!.replace(/^https?:\/\//, '')}
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

                                {showResend && (
                                    <div className="rounded-xl border border-warning/40 bg-warning/10 p-4">
                                        <div className="flex items-center justify-between gap-4">
                                            <div>
                                                <p className="font-medium text-ink">Email not verified</p>
                                                {verificationRequired && (
                                                    <p className="text-sm text-muted">Please verify your email to access all features.</p>
                                                )}
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
                                    disabled={registeringPasskey || passkeyNeedsVerification}
                                    aria-describedby={passkeyNeedsVerification ? 'passkey-verify-hint' : undefined}
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
                            {passkeyNeedsVerification && (
                                <p id="passkey-verify-hint" className="mt-3 text-xs text-warning">
                                    Verify your email to add a passkey.
                                </p>
                            )}
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

                {/* Notifications — only while this instance sends expiry reminders */}
                {appSettings?.link_expiry_reminders_enabled && (
                    <motion.section
                        initial={{ opacity: 0, y: 16 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.12 }}
                        className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
                    >
                        <div className="p-6 border-b border-line">
                            <div className="flex items-center gap-3">
                                <div className="flex h-10 w-10 items-center justify-center rounded-full border border-line bg-paper">
                                    <Bell className="h-5 w-5 text-muted" />
                                </div>
                                <div>
                                    <h2 className="font-display text-lg font-bold text-ink tracking-tight">Notifications</h2>
                                    <p className="text-sm text-muted">Email about your links</p>
                                </div>
                            </div>
                        </div>
                        <div className="p-6">
                            <div className="flex items-start gap-2.5">
                                <input
                                    id="expiry-reminders"
                                    type="checkbox"
                                    checked={profile.notify_link_expiry}
                                    onChange={(e) => handleToggleExpiryReminders(e.target.checked)}
                                    disabled={savingReminders}
                                    aria-describedby="expiry-reminders-hint"
                                    className="mt-0.5 h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500 disabled:opacity-50"
                                />
                                <div>
                                    <label htmlFor="expiry-reminders" className="text-sm font-medium text-ink cursor-pointer">
                                        Email me 3 days before a link expires
                                    </label>
                                    <p id="expiry-reminders-hint" className="mt-0.5 text-xs text-faint">
                                        At most one email a day, listing your links that expire within 3 days. Links that belong to an organization are not included.
                                    </p>
                                    {remindersNeedVerification && (
                                        <p className="mt-2 text-xs text-warning">
                                            Verify your email to get reminders.
                                        </p>
                                    )}
                                </div>
                            </div>
                        </div>
                    </motion.section>
                )}

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
                                        <code ref={createdKeyRef} className="flex-1 select-all break-all rounded-md border border-line2 bg-surface px-3 py-2 font-mono text-xs text-ink">{createdApiKey}</code>
                                        <button
                                            type="button"
                                            onClick={handleCopyApiKey}
                                            className="rounded-lg border border-line2 px-3 py-2 text-sm text-muted transition-colors hover:text-ink hover:border-ink/30"
                                        >
                                            Copy
                                        </button>
                                    </div>
                                    {copyFailed && (
                                        <p role="alert" className="mt-2 text-xs text-danger">
                                            Could not copy automatically. The key is selected: press Ctrl+C (Cmd+C on a Mac) to copy it.
                                        </p>
                                    )}
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
                                                <p className="text-xs text-faint">
                                                    {k.scope === 'read_only' ? 'Read-only' : 'Full access'}
                                                    {' · '}
                                                    {k.expired ? (
                                                        <span className="text-danger">Expired</span>
                                                    ) : k.expires_at ? (
                                                        `Expires ${formatDate(k.expires_at)}`
                                                    ) : (
                                                        'Never expires'
                                                    )}
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

                            <form onSubmit={handleCreateApiKey} className="space-y-3">
                                <input
                                    type="text"
                                    value={newKeyName}
                                    onChange={e => setNewKeyName(e.target.value)}
                                    placeholder="Key name (e.g. MCP on my laptop)"
                                    maxLength={60}
                                    aria-label="Key name"
                                    className={inputClass}
                                />
                                <div className="flex flex-col gap-3 sm:flex-row sm:items-end">
                                    <div className="sm:w-48">
                                        <label htmlFor="api-key-access" className="mb-1.5 block text-sm font-medium text-ink">Access</label>
                                        <select
                                            id="api-key-access"
                                            value={keyScope}
                                            onChange={e => setKeyScope(e.target.value as ApiKeyScopeChoice)}
                                            className={inputClass}
                                        >
                                            <option value="full">Full access</option>
                                            <option value="read_only">Read-only</option>
                                        </select>
                                    </div>
                                    <div className="sm:w-48">
                                        <label htmlFor="api-key-expires" className="mb-1.5 block text-sm font-medium text-ink">Expires</label>
                                        <select
                                            id="api-key-expires"
                                            value={keyExpiry}
                                            onChange={e => setKeyExpiry(e.target.value as ApiKeyExpiryChoice)}
                                            className={inputClass}
                                        >
                                            <option value="never">Never</option>
                                            <option value="30">30 days</option>
                                            <option value="90">90 days</option>
                                            <option value="365">1 year</option>
                                        </select>
                                    </div>
                                    <button
                                        type="submit"
                                        disabled={creatingKey}
                                        className="inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                                    >
                                        {creatingKey ? <Loader2 className="h-4 w-4 animate-spin" /> : <><Plus className="h-4 w-4" /> Create key</>}
                                    </button>
                                </div>
                            </form>
                            <p className="text-xs text-faint">
                                Use with the{' '}
                                <a href="https://github.com/ysalitrynskyi/opn-mcp" target="_blank" rel="noreferrer" className="text-primary-600 underline decoration-primary-300 underline-offset-2 hover:decoration-primary-600">opn.onl MCP server</a>{' '}
                                or any API client: <code className="font-mono">Authorization: Bearer opn_…</code>
                            </p>
                        </div>
                    </motion.section>
                )}

                {/* Public profile (Link-in-Bio) — only shown when the instance enables it */}
                {appSettings?.link_in_bio_enabled && (
                    <BioSettingsSection
                        profile={profile}
                        onProfile={setProfile}
                        onError={setError}
                        onSuccess={setSuccess}
                        inputClass={inputClass}
                        labelClass={labelClass}
                    />
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
                                    <p className="text-sm text-danger">Irreversible actions</p>
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
                                        <p className="text-sm font-medium text-danger">Your account closes as soon as you confirm.</p>
                                        <p className="text-sm text-danger">
                                            You are signed out and your short links stop working right away. Links you
                                            created inside an organization belong to it and keep working.
                                        </p>
                                        <p className="mt-2 text-sm text-danger">{retentionNotice(appSettings.account_purge_days)}</p>
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
