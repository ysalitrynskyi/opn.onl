import React from 'react';
import { motion } from 'framer-motion';
import { Shield, Eye, Database, Lock, Trash2, Mail } from 'lucide-react';
import SEO from '../components/SEO';

export default function Privacy() {
    return (
        <div className="pb-24">
            <SEO
                title="Privacy Policy"
                description="How opn.onl handles your data — privacy-first, no cross-site tracking, no third-party pixels."
                url="/privacy"
            />
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
                        <p className="text-slate-500">Last updated: July 7, 2026</p>
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

                            <h4 className="font-semibold text-slate-900 mb-2">Analytics Data (collected when someone opens a short link)</h4>
                            <ul className="list-disc pl-5 space-y-1">
                                <li>Click counts and timestamps</li>
                                <li>
                                    A <strong>truncated</strong> IP address — we remove the last part of the
                                    address before storing it (IPv4: last octet zeroed; IPv6: reduced to /48).
                                    The full address is used only in memory to derive an approximate location
                                    and for rate limiting, then discarded.
                                </li>
                                <li>
                                    Approximate, city-level location (country, region, city and city-level
                                    coordinates) derived from the IP via a local geolocation database — your
                                    IP is never sent to a third party for this
                                </li>
                                <li>Browser, device type and operating system (parsed from the user agent, which is also stored temporarily — see retention below)</li>
                                <li>Referrer URLs</li>
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
                                <li>
                                    <strong>Per-visitor analytics identifiers:</strong> The truncated IP address
                                    and raw user-agent string of a click are automatically anonymized after
                                    13 months by a scheduled job
                                </li>
                                <li>
                                    <strong>Aggregate analytics:</strong> Non-identifying dimensions (click counts,
                                    country, city, device type, browser, referrer) are retained for the lifetime
                                    of the link so your statistics keep working
                                </li>
                                <li><strong>Backups:</strong> Encrypted database backups are rotated on a fixed schedule; deleted data ages out of backups as they rotate</li>
                            </ul>
                            <h4 className="font-semibold text-slate-900 mb-2">Your rights</h4>
                            <p className="mb-4">
                                Depending on where you live (e.g. under the GDPR or similar laws), you have the
                                right to access, correct, export, object to the processing of, and delete your
                                personal data. Where self-service deletion is enabled you can delete your account
                                from Settings; otherwise email us and we will process verified deletion requests
                                within 30 days.
                            </p>
                            <p>
                                Legal bases we rely on: performance of a contract (providing the service you sign
                                up for), and legitimate interest (abuse prevention, security, and aggregate,
                                non-identifying analytics).
                            </p>
                        </PolicySection>

                        <PolicySection
                            icon={<Shield className="h-5 w-5" />}
                            title="Cookies & Local Storage"
                        >
                            <p className="mb-4">
                                We use your browser's local storage to keep you signed in (your session token).
                                We do not use advertising cookies, cross-site tracking, or third-party analytics
                                or tracking pixels on this site.
                            </p>
                        </PolicySection>

                        <PolicySection
                            icon={<Database className="h-5 w-5" />}
                            title="Service Providers We Rely On"
                        >
                            <p className="mb-4">
                                We don't sell or share your data for marketing. The following infrastructure
                                providers process data on our behalf, strictly to run the service:
                            </p>
                            <ul className="list-disc pl-5 space-y-2">
                                <li><strong>Cloudflare</strong> — network proxy, TLS and DDoS protection in front of our servers (sees connection metadata like your IP, as any network carrier does)</li>
                                <li><strong>MaxMind GeoLite2</strong> — the IP-to-city database used for geographic analytics; lookups run entirely on our own servers, so your IP is never transmitted to MaxMind</li>
                                <li><strong>Email delivery provider</strong> — sends transactional email only (verification, password reset, security notices) to the address you registered</li>
                                <li><strong>Object storage (S3-compatible)</strong> — stores encrypted database backups</li>
                            </ul>
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
                            This privacy policy is effective as of July 7, 2026. We may update this policy
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
