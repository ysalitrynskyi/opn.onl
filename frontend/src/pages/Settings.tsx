import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { 
    Key, Shield, Download, 
    ChevronRight, Loader2, Check, AlertTriangle,
    Fingerprint, Plus
} from 'lucide-react';
import { API_ENDPOINTS } from '../config/api';

export default function Settings() {
    const navigate = useNavigate();
    const [loading, setLoading] = useState(true);
    const [registeringPasskey, setRegisteringPasskey] = useState(false);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');

    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        setLoading(false);
        // In a real app, fetch user data and passkeys here
    }, [navigate]);

    const handleRegisterPasskey = async () => {
        setRegisteringPasskey(true);
        setError('');
        setSuccess('');

        try {
            // Get email from JWT token (in real app, decode or fetch from API)
            const token = localStorage.getItem('token');
            if (!token) return;
            
            // Decode JWT to get email (simple base64 decode of payload)
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
                    excludeCredentials: options.publicKey.excludeCredentials?.map((c: any) => ({
                        ...c,
                        id: base64ToBuffer(c.id),
                    })),
                },
            }) as PublicKeyCredential;

            if (!credential) {
                throw new Error('Credential creation cancelled');
            }

            const response = credential.response as AuthenticatorAttestationResponse;

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
                            attestationObject: bufferToBase64(response.attestationObject),
                            clientDataJSON: bufferToBase64(response.clientDataJSON),
                        },
                        type: credential.type,
                    },
                }),
            });

            if (finishRes.ok) {
                setSuccess('Passkey registered successfully!');
            } else {
                throw new Error('Failed to complete passkey registration');
            }
        } catch (err: any) {
            console.error('Passkey registration error:', err);
            setError(err.message || 'Failed to register passkey. Make sure your browser supports passkeys.');
        } finally {
            setRegisteringPasskey(false);
        }
    };

    const handleExportData = async () => {
        window.open(API_ENDPOINTS.exportLinks, '_blank');
    };

    const handleDeleteAccount = async () => {
        const confirmed = window.confirm(
            'Are you sure you want to delete your account? This action cannot be undone and all your links will be permanently deleted.'
        );
        
        if (confirmed) {
            // In a real app, call delete account API
            alert('Account deletion is not implemented in this demo.');
        }
    };

    if (loading) {
        return (
            <div className="flex justify-center py-20">
                <Loader2 className="h-8 w-8 animate-spin text-primary-600" />
            </div>
        );
    }

    return (
        <div className="max-w-3xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
            >
                <h1 className="text-3xl font-bold text-slate-900 mb-2">Settings</h1>
                <p className="text-slate-500 mb-8">Manage your account and preferences</p>

                {error && (
                    <motion.div
                        initial={{ opacity: 0, y: -10 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="mb-6 p-4 bg-red-50 border border-red-200 rounded-xl text-red-700 flex items-center gap-3"
                    >
                        <AlertTriangle className="h-5 w-5 flex-shrink-0" />
                        {error}
                    </motion.div>
                )}

                {success && (
                    <motion.div
                        initial={{ opacity: 0, y: -10 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="mb-6 p-4 bg-emerald-50 border border-emerald-200 rounded-xl text-emerald-700 flex items-center gap-3"
                    >
                        <Check className="h-5 w-5 flex-shrink-0" />
                        {success}
                    </motion.div>
                )}

                <div className="space-y-6">
                    {/* Security Section */}
                    <section className="bg-white rounded-xl border border-slate-200 shadow-sm overflow-hidden">
                        <div className="px-6 py-4 border-b border-slate-100">
                            <h2 className="text-lg font-semibold text-slate-900 flex items-center gap-2">
                                <Shield className="h-5 w-5 text-slate-400" />
                                Security
                            </h2>
                        </div>
                        <div className="divide-y divide-slate-100">
                            {/* Passkeys */}
                            <div className="px-6 py-4">
                                <div className="flex items-center justify-between">
                                    <div className="flex items-center gap-4">
                                        <div className="h-10 w-10 bg-primary-100 rounded-lg flex items-center justify-center">
                                            <Fingerprint className="h-5 w-5 text-primary-600" />
                                        </div>
                                        <div>
                                            <h3 className="font-medium text-slate-900">Passkeys</h3>
                                            <p className="text-sm text-slate-500">
                                                Sign in securely without a password
                                            </p>
                                        </div>
                                    </div>
                                    <button
                                        onClick={handleRegisterPasskey}
                                        disabled={registeringPasskey}
                                        className="inline-flex items-center gap-2 px-4 py-2 bg-primary-600 text-white rounded-lg font-medium hover:bg-primary-700 disabled:opacity-70"
                                    >
                                        {registeringPasskey ? (
                                            <Loader2 className="h-4 w-4 animate-spin" />
                                        ) : (
                                            <Plus className="h-4 w-4" />
                                        )}
                                        Add Passkey
                                    </button>
                                </div>
                                <p className="mt-3 text-xs text-slate-400">
                                    Passkeys use your device's biometrics or security key for secure, passwordless authentication.
                                </p>
                            </div>

                            {/* Change Password */}
                            <button className="w-full px-6 py-4 flex items-center justify-between hover:bg-slate-50 transition-colors">
                                <div className="flex items-center gap-4">
                                    <div className="h-10 w-10 bg-slate-100 rounded-lg flex items-center justify-center">
                                        <Key className="h-5 w-5 text-slate-500" />
                                    </div>
                                    <div className="text-left">
                                        <h3 className="font-medium text-slate-900">Change Password</h3>
                                        <p className="text-sm text-slate-500">Update your account password</p>
                                    </div>
                                </div>
                                <ChevronRight className="h-5 w-5 text-slate-400" />
                            </button>
                        </div>
                    </section>

                    {/* Data Section */}
                    <section className="bg-white rounded-xl border border-slate-200 shadow-sm overflow-hidden">
                        <div className="px-6 py-4 border-b border-slate-100">
                            <h2 className="text-lg font-semibold text-slate-900 flex items-center gap-2">
                                <Download className="h-5 w-5 text-slate-400" />
                                Data & Export
                            </h2>
                        </div>
                        <div className="divide-y divide-slate-100">
                            <button 
                                onClick={handleExportData}
                                className="w-full px-6 py-4 flex items-center justify-between hover:bg-slate-50 transition-colors"
                            >
                                <div className="flex items-center gap-4">
                                    <div className="h-10 w-10 bg-emerald-100 rounded-lg flex items-center justify-center">
                                        <Download className="h-5 w-5 text-emerald-600" />
                                    </div>
                                    <div className="text-left">
                                        <h3 className="font-medium text-slate-900">Export Links</h3>
                                        <p className="text-sm text-slate-500">Download all your links as CSV</p>
                                    </div>
                                </div>
                                <ChevronRight className="h-5 w-5 text-slate-400" />
                            </button>
                        </div>
                    </section>

                    {/* Danger Zone */}
                    <section className="bg-white rounded-xl border border-red-200 shadow-sm overflow-hidden">
                        <div className="px-6 py-4 border-b border-red-100 bg-red-50">
                            <h2 className="text-lg font-semibold text-red-900 flex items-center gap-2">
                                <AlertTriangle className="h-5 w-5 text-red-500" />
                                Danger Zone
                            </h2>
                        </div>
                        <div className="px-6 py-4">
                            <div className="flex items-center justify-between">
                                <div>
                                    <h3 className="font-medium text-slate-900">Delete Account</h3>
                                    <p className="text-sm text-slate-500">
                                        Permanently delete your account and all associated data
                                    </p>
                                </div>
                                <button
                                    onClick={handleDeleteAccount}
                                    className="px-4 py-2 border border-red-300 text-red-600 rounded-lg font-medium hover:bg-red-50 transition-colors"
                                >
                                    Delete Account
                                </button>
                            </div>
                        </div>
                    </section>
                </div>
            </motion.div>
        </div>
    );
}

// Helper functions for WebAuthn
function base64ToBuffer(base64: string): ArrayBuffer {
    // Handle base64url encoding
    const base64Standard = base64.replace(/-/g, '+').replace(/_/g, '/');
    const padding = '='.repeat((4 - base64Standard.length % 4) % 4);
    const binary = atob(base64Standard + padding);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    return bytes.buffer;
}

function bufferToBase64(buffer: ArrayBuffer): string {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    // Use base64url encoding
    return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

