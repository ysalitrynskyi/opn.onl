import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { 
    Key, Shield, Download, Trash2,
    ChevronRight, Loader2, Check, AlertTriangle,
    Fingerprint, Plus, User, Edit2, X
} from 'lucide-react';
import { API_ENDPOINTS, getAuthHeaders } from '../config/api';
import SEO from '../components/SEO';

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
}

interface AppSettings {
    account_deletion_enabled: boolean;
    custom_aliases_enabled: boolean;
    max_links_per_user: number | null;
    passkeys_enabled: boolean;
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

    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        fetchData();
    }, [navigate]);

    const fetchData = async () => {
        try {
            setLoading(true);
            const [profileRes, settingsRes, passkeysRes] = await Promise.all([
                fetch(API_ENDPOINTS.userProfile, { headers: getAuthHeaders() }),
                fetch(API_ENDPOINTS.appSettings),
                fetch(API_ENDPOINTS.passkeys, { headers: getAuthHeaders() }),
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
        } catch (err) {
            console.error('Failed to fetch settings data', err);
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

            // Step 1: Start registration
            const startRes = await fetch(API_ENDPOINTS.passkeyRegisterStart, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
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
                    excludeCredentials: options.publicKey.excludeCredentials?.map((cred: any) => ({
                        ...cred,
                        id: base64ToBuffer(cred.id),
                    })),
                },
            }) as PublicKeyCredential;

            if (!credential) {
                throw new Error('No credential returned');
            }

            const attestation = credential.response as AuthenticatorAttestationResponse;

            // Step 3: Finish registration
            const finishRes = await fetch(API_ENDPOINTS.passkeyRegisterFinish, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
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
        } catch (err: any) {
            setError(err.message || 'Failed to register passkey');
        } finally {
            setRegisteringPasskey(false);
        }
    };

    const handleDeletePasskey = async (passkeyId: number) => {
        if (!confirm('Are you sure you want to delete this passkey?')) return;
        
        try {
            const res = await fetch(API_ENDPOINTS.passkeyDelete, {
                method: 'POST',
                headers: getAuthHeaders(),
                body: JSON.stringify({ passkey_id: passkeyId }),
            });
            
            if (!res.ok) {
                const data = await res.json();
                throw new Error(data.error || 'Failed to delete passkey');
            }
            
            setSuccess('Passkey deleted successfully');
            fetchData();
        } catch (err: any) {
            setError(err.message);
        }
    };

    const handleRenamePasskey = async (passkeyId: number) => {
        if (!newPasskeyName.trim()) return;
        
        try {
            const res = await fetch(API_ENDPOINTS.passkeyRename, {
                method: 'POST',
                headers: getAuthHeaders(),
                body: JSON.stringify({ passkey_id: passkeyId, name: newPasskeyName }),
            });
            
            if (!res.ok) throw new Error('Failed to rename passkey');
            
            setRenamingPasskeyId(null);
            setNewPasskeyName('');
            fetchData();
        } catch (err: any) {
            setError(err.message);
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
            const res = await fetch(API_ENDPOINTS.changePassword, {
                method: 'POST',
                headers: getAuthHeaders(),
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
        } catch (err: any) {
            setError(err.message);
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
            const res = await fetch(API_ENDPOINTS.deleteAccount, {
                method: 'POST',
                headers: getAuthHeaders(),
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
        } catch (err: any) {
            setError(err.message);
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
        } catch (err: any) {
            setError(err.message);
        } finally {
            setResendingVerification(false);
        }
    };

    const handleExportData = async () => {
        try {
            const response = await fetch(API_ENDPOINTS.exportLinks, {
                headers: getAuthHeaders(),
            });
            
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
        } catch (err: any) {
            setError(err.message);
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
            const res = await fetch(API_ENDPOINTS.updateProfile, {
                method: 'PUT',
                headers: getAuthHeaders(),
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
        } catch (err: any) {
            setError(err.message);
        } finally {
            setSavingProfile(false);
        }
    };

    if (loading) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center">
                <Loader2 className="h-8 w-8 animate-spin text-primary-600" />
            </div>
        );
    }

    return (
        <div className="max-w-3xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
            <SEO title="Settings" description="Manage your account settings" noIndex />
            
            <h1 className="text-3xl font-bold text-slate-900 mb-8">Settings</h1>

            {error && (
                <motion.div 
                    initial={{ opacity: 0, y: -10 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="mb-6 p-4 bg-red-50 border border-red-200 rounded-xl text-red-700 text-sm flex items-center gap-2"
                >
                    <AlertTriangle className="h-4 w-4 flex-shrink-0" />
                    {error}
                    <button onClick={() => setError('')} className="ml-auto">
                        <X className="h-4 w-4" />
                    </button>
                </motion.div>
            )}

            {success && (
                <motion.div 
                    initial={{ opacity: 0, y: -10 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="mb-6 p-4 bg-green-50 border border-green-200 rounded-xl text-green-700 text-sm flex items-center gap-2"
                >
                    <Check className="h-4 w-4 flex-shrink-0" />
                    {success}
                    <button onClick={() => setSuccess('')} className="ml-auto">
                        <X className="h-4 w-4" />
                    </button>
                </motion.div>
            )}

            <div className="space-y-6">
                {/* Profile Section */}
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="bg-white rounded-2xl shadow-sm border border-slate-200 overflow-hidden"
                >
                    <div className="p-6 border-b border-slate-100">
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-3">
                                <div className="h-10 w-10 bg-primary-100 rounded-full flex items-center justify-center">
                                    <User className="h-5 w-5 text-primary-600" />
                                </div>
                                <div>
                                    <h2 className="text-lg font-semibold text-slate-900">
                                        {profile?.display_name || 'Profile'}
                                    </h2>
                                    <p className="text-sm text-slate-500">{profile?.email}</p>
                                </div>
                            </div>
                            <div className="flex items-center gap-2">
                                {profile?.email_verified ? (
                                    <span className="inline-flex items-center gap-1 px-2.5 py-1 bg-green-100 text-green-700 rounded-full text-xs font-medium">
                                        <Check className="h-3 w-3" />
                                        Verified
                                    </span>
                                ) : (
                                    <span className="inline-flex items-center gap-1 px-2.5 py-1 bg-amber-100 text-amber-700 rounded-full text-xs font-medium">
                                        <AlertTriangle className="h-3 w-3" />
                                        Unverified
                                    </span>
                                )}
                                {!editingProfile && (
                                    <button
                                        onClick={handleEditProfile}
                                        className="p-2 text-slate-400 hover:text-slate-600"
                                        title="Edit profile"
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
                                    <label className="block text-sm font-medium text-slate-700 mb-1">Display Name</label>
                                    <input
                                        type="text"
                                        value={displayName}
                                        onChange={(e) => setDisplayName(e.target.value)}
                                        className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                                        placeholder="Your name"
                                        maxLength={100}
                                    />
                                </div>
                                <div>
                                    <label className="block text-sm font-medium text-slate-700 mb-1">Bio</label>
                                    <textarea
                                        value={bio}
                                        onChange={(e) => setBio(e.target.value)}
                                        className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-transparent resize-none"
                                        placeholder="Tell us about yourself"
                                        rows={3}
                                        maxLength={500}
                                    />
                                    <p className="text-xs text-slate-500 mt-1">{bio.length}/500 characters</p>
                                </div>
                                <div className="grid grid-cols-2 gap-4">
                                    <div>
                                        <label className="block text-sm font-medium text-slate-700 mb-1">Website</label>
                                        <input
                                            type="url"
                                            value={website}
                                            onChange={(e) => setWebsite(e.target.value)}
                                            className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                                            placeholder="https://example.com"
                                        />
                                    </div>
                                    <div>
                                        <label className="block text-sm font-medium text-slate-700 mb-1">Location</label>
                                        <input
                                            type="text"
                                            value={location}
                                            onChange={(e) => setLocation(e.target.value)}
                                            className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                                            placeholder="City, Country"
                                            maxLength={100}
                                        />
                                    </div>
                                </div>
                                <div className="flex gap-3 pt-2">
                                    <button
                                        type="button"
                                        onClick={() => setEditingProfile(false)}
                                        className="px-4 py-2 text-slate-600 hover:text-slate-800"
                                    >
                                        Cancel
                                    </button>
                                    <button
                                        type="submit"
                                        disabled={savingProfile}
                                        className="px-4 py-2 bg-primary-600 text-white rounded-lg font-medium hover:bg-primary-700 disabled:opacity-50"
                                    >
                                        {savingProfile ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Save Profile'}
                                    </button>
                                </div>
                            </form>
                        ) : (
                            <>
                                {/* Profile info display */}
                                {(profile?.bio || profile?.website || profile?.location) && (
                                    <div className="space-y-2 pb-4 border-b border-slate-100">
                                        {profile?.bio && (
                                            <p className="text-slate-600">{profile.bio}</p>
                                        )}
                                        {(profile?.website || profile?.location) && (
                                            <div className="flex flex-wrap gap-4 text-sm text-slate-500">
                                                {profile?.website && (
                                                    <a href={profile.website} target="_blank" rel="noreferrer" className="hover:text-primary-600">
                                                        🔗 {profile.website.replace(/^https?:\/\//, '')}
                                                    </a>
                                                )}
                                                {profile?.location && (
                                                    <span>📍 {profile.location}</span>
                                                )}
                                            </div>
                                        )}
                                    </div>
                                )}
                                
                                <div className="grid grid-cols-2 gap-4">
                                    <div className="p-4 bg-slate-50 rounded-xl">
                                        <p className="text-sm text-slate-500">Total Links</p>
                                        <p className="text-2xl font-bold text-slate-900">{profile?.link_count?.toLocaleString() || 0}</p>
                                    </div>
                                    <div className="p-4 bg-slate-50 rounded-xl">
                                        <p className="text-sm text-slate-500">Total Clicks</p>
                                        <p className="text-2xl font-bold text-slate-900">{profile?.total_clicks?.toLocaleString() || 0}</p>
                                    </div>
                                </div>
                                
                                {/* Show resend verification only if NOT verified */}
                                {profile && !profile.email_verified && (
                                    <div className="p-4 bg-amber-50 border border-amber-200 rounded-xl">
                                        <div className="flex items-center justify-between">
                                            <div>
                                                <p className="font-medium text-amber-800">Email not verified</p>
                                                <p className="text-sm text-amber-700">Please verify your email to access all features.</p>
                                            </div>
                                            <button
                                                onClick={handleResendVerification}
                                                disabled={resendingVerification}
                                                className="px-4 py-2 bg-amber-600 text-white rounded-lg text-sm font-medium hover:bg-amber-700 disabled:opacity-50"
                                            >
                                                {resendingVerification ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Resend'}
                                            </button>
                                        </div>
                                    </div>
                                )}
                            </>
                        )}
                    </div>
                </motion.div>

                {/* Passkeys Section */}
                {appSettings?.passkeys_enabled && (
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.1 }}
                        className="bg-white rounded-2xl shadow-sm border border-slate-200 overflow-hidden"
                    >
                        <div className="p-6 border-b border-slate-100">
                            <div className="flex items-center justify-between">
                                <div className="flex items-center gap-3">
                                    <div className="h-10 w-10 bg-indigo-100 rounded-full flex items-center justify-center">
                                        <Fingerprint className="h-5 w-5 text-indigo-600" />
                                    </div>
                                    <div>
                                        <h2 className="text-lg font-semibold text-slate-900">Passkeys</h2>
                                        <p className="text-sm text-slate-500">Passwordless authentication</p>
                                    </div>
                                </div>
                                <button
                                    onClick={handleRegisterPasskey}
                                    disabled={registeringPasskey}
                                    className="inline-flex items-center gap-2 px-4 py-2 bg-indigo-600 text-white rounded-lg text-sm font-medium hover:bg-indigo-700 disabled:opacity-50"
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
                        <div className="divide-y divide-slate-100">
                            {passkeys.length === 0 ? (
                                <div className="p-6 text-center text-slate-500">
                                    <Fingerprint className="h-12 w-12 text-slate-300 mx-auto mb-3" />
                                    <p>No passkeys registered</p>
                                    <p className="text-sm">Add a passkey for passwordless login</p>
                                </div>
                            ) : (
                                passkeys.map((passkey) => (
                                    <div key={passkey.id} className="p-4 flex items-center justify-between">
                                        <div className="flex items-center gap-3">
                                            <Key className="h-5 w-5 text-slate-400" />
                                            <div>
                                                {renamingPasskeyId === passkey.id ? (
                                                    <div className="flex items-center gap-2">
                                                        <input
                                                            type="text"
                                                            value={newPasskeyName}
                                                            onChange={(e) => setNewPasskeyName(e.target.value)}
                                                            className="px-2 py-1 border border-slate-300 rounded text-sm"
                                                            placeholder="Passkey name"
                                                            autoFocus
                                                        />
                                                        <button
                                                            onClick={() => handleRenamePasskey(passkey.id)}
                                                            className="text-green-600 hover:text-green-700"
                                                        >
                                                            <Check className="h-4 w-4" />
                                                        </button>
                                                        <button
                                                            onClick={() => { setRenamingPasskeyId(null); setNewPasskeyName(''); }}
                                                            className="text-slate-400 hover:text-slate-600"
                                                        >
                                                            <X className="h-4 w-4" />
                                                        </button>
                                                    </div>
                                                ) : (
                                                    <p className="font-medium text-slate-900">{passkey.name}</p>
                                                )}
                                                <p className="text-xs text-slate-500">
                                                    Created {new Date(passkey.created_at).toLocaleDateString()}
                                                    {passkey.last_used && ` · Last used ${new Date(passkey.last_used).toLocaleDateString()}`}
                                                </p>
                                            </div>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            <button
                                                onClick={() => { setRenamingPasskeyId(passkey.id); setNewPasskeyName(passkey.name); }}
                                                className="p-2 text-slate-400 hover:text-slate-600"
                                                title="Rename"
                                            >
                                                <Edit2 className="h-4 w-4" />
                                            </button>
                                            <button
                                                onClick={() => handleDeletePasskey(passkey.id)}
                                                className="p-2 text-slate-400 hover:text-red-600"
                                                title="Delete"
                                            >
                                                <Trash2 className="h-4 w-4" />
                                            </button>
                                        </div>
                                    </div>
                                ))
                            )}
                        </div>
                    </motion.div>
                )}

                {/* Security Section */}
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.2 }}
                    className="bg-white rounded-2xl shadow-sm border border-slate-200 overflow-hidden"
                >
                    <div className="p-6 border-b border-slate-100">
                        <div className="flex items-center gap-3">
                            <div className="h-10 w-10 bg-emerald-100 rounded-full flex items-center justify-center">
                                <Shield className="h-5 w-5 text-emerald-600" />
                            </div>
                            <div>
                                <h2 className="text-lg font-semibold text-slate-900">Security</h2>
                                <p className="text-sm text-slate-500">Manage your password</p>
                            </div>
                        </div>
                    </div>
                    <div className="p-6">
                        {!showChangePassword ? (
                            <button
                                onClick={() => setShowChangePassword(true)}
                                className="flex items-center justify-between w-full p-4 bg-slate-50 rounded-xl hover:bg-slate-100 transition-colors"
                            >
                                <div className="flex items-center gap-3">
                                    <Key className="h-5 w-5 text-slate-400" />
                                    <span className="font-medium text-slate-700">Change Password</span>
                                </div>
                                <ChevronRight className="h-5 w-5 text-slate-400" />
                            </button>
                        ) : (
                            <form onSubmit={handleChangePassword} className="space-y-4">
                                <input
                                    type="password"
                                    placeholder="Current password"
                                    value={currentPassword}
                                    onChange={(e) => setCurrentPassword(e.target.value)}
                                    className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                                    required
                                />
                                <input
                                    type="password"
                                    placeholder="New password (min 8 characters)"
                                    value={newPassword}
                                    onChange={(e) => setNewPassword(e.target.value)}
                                    className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                                    required
                                    minLength={8}
                                />
                                <input
                                    type="password"
                                    placeholder="Confirm new password"
                                    value={confirmPassword}
                                    onChange={(e) => setConfirmPassword(e.target.value)}
                                    className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500 focus:border-transparent"
                                    required
                                />
                                <div className="flex gap-3">
                                    <button
                                        type="button"
                                        onClick={() => setShowChangePassword(false)}
                                        className="px-4 py-2 text-slate-600 hover:text-slate-800"
                                    >
                                        Cancel
                                    </button>
                                    <button
                                        type="submit"
                                        disabled={changingPassword}
                                        className="px-4 py-2 bg-primary-600 text-white rounded-lg font-medium hover:bg-primary-700 disabled:opacity-50"
                                    >
                                        {changingPassword ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Update Password'}
                                    </button>
                                </div>
                            </form>
                        )}
                    </div>
                </motion.div>

                {/* Data Section */}
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.3 }}
                    className="bg-white rounded-2xl shadow-sm border border-slate-200 overflow-hidden"
                >
                    <div className="p-6 border-b border-slate-100">
                        <div className="flex items-center gap-3">
                            <div className="h-10 w-10 bg-blue-100 rounded-full flex items-center justify-center">
                                <Download className="h-5 w-5 text-blue-600" />
                            </div>
                            <div>
                                <h2 className="text-lg font-semibold text-slate-900">Data</h2>
                                <p className="text-sm text-slate-500">Export your data</p>
                            </div>
                        </div>
                    </div>
                    <div className="p-6">
                        <button
                            onClick={handleExportData}
                            className="flex items-center justify-between w-full p-4 bg-slate-50 rounded-xl hover:bg-slate-100 transition-colors"
                        >
                            <div className="flex items-center gap-3">
                                <Download className="h-5 w-5 text-slate-400" />
                                <span className="font-medium text-slate-700">Export All Links (CSV)</span>
                            </div>
                            <ChevronRight className="h-5 w-5 text-slate-400" />
                        </button>
                    </div>
                </motion.div>

                {/* Danger Zone - Only show if account deletion is enabled */}
                {appSettings?.account_deletion_enabled && (
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.4 }}
                        className="bg-white rounded-2xl shadow-sm border border-red-200 overflow-hidden"
                    >
                        <div className="p-6 border-b border-red-100 bg-red-50">
                            <div className="flex items-center gap-3">
                                <div className="h-10 w-10 bg-red-100 rounded-full flex items-center justify-center">
                                    <Trash2 className="h-5 w-5 text-red-600" />
                                </div>
                                <div>
                                    <h2 className="text-lg font-semibold text-red-900">Danger Zone</h2>
                                    <p className="text-sm text-red-700">Irreversible actions</p>
                                </div>
                            </div>
                        </div>
                        <div className="p-6">
                            {!showDeleteAccount ? (
                                <button
                                    onClick={() => setShowDeleteAccount(true)}
                                    className="flex items-center justify-between w-full p-4 bg-red-50 rounded-xl hover:bg-red-100 transition-colors border border-red-200"
                                >
                                    <div className="flex items-center gap-3">
                                        <Trash2 className="h-5 w-5 text-red-500" />
                                        <span className="font-medium text-red-700">Delete Account</span>
                                    </div>
                                    <ChevronRight className="h-5 w-5 text-red-400" />
                                </button>
                            ) : (
                                <form onSubmit={handleDeleteAccount} className="space-y-4">
                                    <div className="p-4 bg-red-50 border border-red-200 rounded-xl">
                                        <p className="text-red-800 text-sm font-medium">This action cannot be undone!</p>
                                        <p className="text-red-700 text-sm">All your links and data will be permanently deleted.</p>
                                    </div>
                                    <input
                                        type="password"
                                        placeholder="Enter your password to confirm"
                                        value={deletePassword}
                                        onChange={(e) => setDeletePassword(e.target.value)}
                                        className="w-full px-4 py-2 border border-red-300 rounded-lg focus:ring-2 focus:ring-red-500 focus:border-transparent"
                                        required
                                    />
                                    <div className="flex gap-3">
                                        <button
                                            type="button"
                                            onClick={() => setShowDeleteAccount(false)}
                                            className="px-4 py-2 text-slate-600 hover:text-slate-800"
                                        >
                                            Cancel
                                        </button>
                                        <button
                                            type="submit"
                                            disabled={deletingAccount}
                                            className="px-4 py-2 bg-red-600 text-white rounded-lg font-medium hover:bg-red-700 disabled:opacity-50"
                                        >
                                            {deletingAccount ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Delete My Account'}
                                        </button>
                                    </div>
                                </form>
                            )}
                        </div>
                    </motion.div>
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
