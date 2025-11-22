import React from 'react';
import { motion } from 'framer-motion';
import { Shield, Eye, Database, Lock, Trash2, Mail } from 'lucide-react';

export default function Privacy() {
    return (
        <div className="pb-24">
            {/* Hero */}
            <section className="bg-gradient-to-b from-slate-50 to-white py-16">
                <div className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="text-center"
                    >
                        <div className="h-16 w-16 bg-emerald-100 rounded-2xl flex items-center justify-center mx-auto mb-6">
                            <Shield className="h-8 w-8 text-emerald-600" />
                        </div>
                        <h1 className="text-4xl font-extrabold text-slate-900 mb-4">Privacy Policy</h1>
                        <p className="text-slate-500">Last updated: December 7, 2025</p>
                    </motion.div>
                </div>
            </section>

            {/* Content */}
            <section className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-12">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.2 }}
                    className="space-y-12"
                >
                    {/* Intro */}
                    <div className="bg-emerald-50 border border-emerald-200 rounded-2xl p-6">
                        <h2 className="text-lg font-bold text-emerald-900 mb-2">Our Privacy Commitment</h2>
                        <p className="text-emerald-800">
                            At opn.onl, privacy isn't just a feature — it's a core value. We collect only what's 
                            necessary to provide our service, and we never sell your data to third parties.
                        </p>
                    </div>

                    {/* Sections */}
                    <div className="space-y-10">
                        <PolicySection
                            icon={<Database className="h-5 w-5" />}
                            title="Information We Collect"
                        >
                            <h4 className="font-semibold text-slate-900 mb-2">Account Information</h4>
                            <ul className="list-disc pl-5 mb-4 space-y-1">
                                <li>Email address (for authentication and account recovery)</li>
                                <li>Password hash (we never store passwords in plain text)</li>
                                <li>Passkey credentials (if you enable passwordless login)</li>
                            </ul>

                            <h4 className="font-semibold text-slate-900 mb-2">Link Data</h4>
                            <ul className="list-disc pl-5 mb-4 space-y-1">
                                <li>Original URLs you shorten</li>
                                <li>Custom aliases you create</li>
                                <li>Creation timestamps</li>
                                <li>Expiration dates (if set)</li>
                            </ul>

                            <h4 className="font-semibold text-slate-900 mb-2">Analytics Data</h4>
                            <ul className="list-disc pl-5 space-y-1">
                                <li>Click counts</li>
                                <li>IP addresses (for geographic analytics)</li>
                                <li>User agent strings (browser/device info)</li>
                                <li>Referrer URLs</li>
                                <li>Click timestamps</li>
                            </ul>
                        </PolicySection>

                        <PolicySection
                            icon={<Eye className="h-5 w-5" />}
                            title="How We Use Your Information"
                        >
                            <p className="mb-4">We use your information exclusively to:</p>
                            <ul className="list-disc pl-5 space-y-2">
                                <li><strong>Provide the service:</strong> Redirect shortened links, manage your account, and display analytics.</li>
                                <li><strong>Prevent abuse:</strong> Detect spam, malicious links, and terms of service violations.</li>
                                <li><strong>Improve the product:</strong> Understand usage patterns to make opn.onl better (in aggregate, never individual).</li>
                                <li><strong>Communicate with you:</strong> Send account-related notifications (password resets, security alerts).</li>
                            </ul>
                            <div className="mt-4 p-4 bg-slate-100 rounded-lg">
                                <p className="text-sm text-slate-700">
                                    <strong>We do NOT:</strong> Sell your data, show you ads, track you across other websites, 
                                    or share your information with third parties except as required by law.
                                </p>
                            </div>
                        </PolicySection>

                        <PolicySection
                            icon={<Lock className="h-5 w-5" />}
                            title="Data Security"
                        >
                            <p className="mb-4">We take security seriously:</p>
                            <ul className="list-disc pl-5 space-y-2">
                                <li>All data is encrypted in transit (HTTPS/TLS)</li>
                                <li>Passwords are hashed using bcrypt with appropriate cost factors</li>
                                <li>Database access is restricted and monitored</li>
                                <li>We support passkey authentication for passwordless security</li>
                                <li>Regular security audits and updates</li>
                            </ul>
                        </PolicySection>

                        <PolicySection
                            icon={<Trash2 className="h-5 w-5" />}
                            title="Data Retention & Deletion"
                        >
                            <p className="mb-4">
                                We retain your data only as long as necessary to provide the service:
                            </p>
                            <ul className="list-disc pl-5 space-y-2 mb-4">
                                <li><strong>Account data:</strong> Retained until you delete your account</li>
                                <li><strong>Link data:</strong> Retained until you delete the link or your account</li>
                                <li><strong>Analytics data:</strong> Retained for the lifetime of the link for analytics purposes</li>
                            </ul>
                            <p>
                                You can request complete deletion of your account and all associated data at any time 
                                by contacting us. We will process deletion requests within 30 days.
                            </p>
                        </PolicySection>

                        <PolicySection
                            icon={<Mail className="h-5 w-5" />}
                            title="Contact Us"
                        >
                            <p>
                                If you have any questions about this Privacy Policy or how we handle your data, 
                                please contact us at:
                            </p>
                            <a 
                                href="mailto:privacy@opn.onl" 
                                className="inline-block mt-4 text-primary-600 font-medium hover:underline"
                            >
                                privacy@opn.onl
                            </a>
                        </PolicySection>
                    </div>

                    {/* Footer */}
                    <div className="border-t border-slate-200 pt-8">
                        <p className="text-sm text-slate-500 text-center">
                            This privacy policy is effective as of December 7, 2025. We may update this policy 
                            from time to time. We will notify you of any significant changes by email or through 
                            a notice on our website.
                        </p>
                    </div>
                </motion.div>
            </section>
        </div>
    );
}

function PolicySection({ 
    icon, 
    title, 
    children 
}: { 
    icon: React.ReactNode; 
    title: string; 
    children: React.ReactNode;
}) {
    return (
        <div>
            <div className="flex items-center gap-3 mb-4">
                <div className="h-10 w-10 bg-slate-100 rounded-lg flex items-center justify-center text-slate-600">
                    {icon}
                </div>
                <h3 className="text-xl font-bold text-slate-900">{title}</h3>
            </div>
            <div className="text-slate-600 leading-relaxed pl-13">
                {children}
            </div>
        </div>
    );
}
