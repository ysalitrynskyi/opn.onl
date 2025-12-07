import { motion } from 'framer-motion';
import { FileText, AlertTriangle } from 'lucide-react';

export default function Terms() {
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
                        <div className="h-16 w-16 bg-primary-100 rounded-2xl flex items-center justify-center mx-auto mb-6">
                            <FileText className="h-8 w-8 text-primary-600" />
                        </div>
                        <h1 className="text-4xl font-extrabold text-slate-900 mb-4">Terms of Service</h1>
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
                    className="space-y-10"
                >
                    {/* Intro */}
                    <div className="prose prose-lg prose-slate">
                        <p>
                            Welcome to opn.onl! By using our service, you agree to these Terms of Service. 
                            Please read them carefully. If you don't agree with these terms, please don't use our service.
                        </p>
                    </div>

                    {/* Sections */}
                    <TermsSection number="1" title="Acceptance of Terms">
                        <p>
                            By accessing or using opn.onl, you agree to be bound by these Terms of Service and all 
                            applicable laws and regulations. If you are using the service on behalf of an organization, 
                            you represent that you have the authority to bind that organization to these terms.
                        </p>
                    </TermsSection>

                    <TermsSection number="2" title="Description of Service">
                        <p className="mb-4">
                            opn.onl provides URL shortening services that allow you to create shortened links, 
                            track click analytics, and manage your links. The service includes:
                        </p>
                        <ul className="list-disc pl-5 space-y-1">
                            <li>Creating shortened URLs with optional custom aliases</li>
                            <li>Viewing click analytics and statistics</li>
                            <li>Password-protecting links</li>
                            <li>Setting link expiration dates</li>
                            <li>Generating QR codes</li>
                            <li>API access for programmatic link creation</li>
                        </ul>
                    </TermsSection>

                    <TermsSection number="3" title="Acceptable Use">
                        <div className="bg-amber-50 border border-amber-200 rounded-xl p-4 mb-4">
                            <div className="flex items-start gap-3">
                                <AlertTriangle className="h-5 w-5 text-amber-600 flex-shrink-0 mt-0.5" />
                                <p className="text-amber-800 text-sm">
                                    <strong>Important:</strong> Violation of these acceptable use policies may result in 
                                    immediate termination of your account without warning.
                                </p>
                            </div>
                        </div>
                        <p className="mb-4">You agree NOT to use opn.onl to:</p>
                        <ul className="list-disc pl-5 space-y-2">
                            <li><strong>Distribute malware:</strong> Links to viruses, trojans, or other malicious software</li>
                            <li><strong>Conduct phishing:</strong> Links designed to deceive users into providing sensitive information</li>
                            <li><strong>Share illegal content:</strong> Links to content that violates any applicable laws</li>
                            <li><strong>Spam:</strong> Mass distribution of unsolicited links</li>
                            <li><strong>Infringe copyrights:</strong> Links to pirated content or copyright-infringing material</li>
                            <li><strong>Harass or abuse:</strong> Links used to harass, threaten, or abuse others</li>
                            <li><strong>Circumvent restrictions:</strong> Using the service to bypass access controls on other platforms</li>
                        </ul>
                    </TermsSection>

                    <TermsSection number="4" title="Account Responsibilities">
                        <ul className="list-disc pl-5 space-y-2">
                            <li>You are responsible for maintaining the security of your account credentials</li>
                            <li>You must provide accurate information when creating an account</li>
                            <li>You are responsible for all activity that occurs under your account</li>
                            <li>You must notify us immediately of any unauthorized use of your account</li>
                            <li>One person or entity may only maintain one free account</li>
                        </ul>
                    </TermsSection>

                    <TermsSection number="5" title="Intellectual Property">
                        <p className="mb-4">
                            The opn.onl service, including its design, code, and branding, is protected by copyright 
                            and other intellectual property laws. opn.onl is open source software licensed 
                            under the AGPL-3.0 License.
                        </p>
                        <p>
                            You retain all rights to the content you link to through our service. By using opn.onl, 
                            you grant us a limited license to process and redirect your links as necessary to provide 
                            the service.
                        </p>
                    </TermsSection>

                    <TermsSection number="6" title="Service Availability">
                        <p>
                            We strive to maintain high availability, but we do not guarantee uninterrupted access 
                            to the service. We may temporarily suspend access for maintenance, updates, or in response 
                            to security concerns. We are not liable for any damages resulting from service interruptions.
                        </p>
                    </TermsSection>

                    <TermsSection number="7" title="Termination">
                        <p className="mb-4">
                            We reserve the right to terminate or suspend your account at any time, with or without 
                            notice, for conduct that we believe:
                        </p>
                        <ul className="list-disc pl-5 space-y-1">
                            <li>Violates these Terms of Service</li>
                            <li>Is harmful to other users or the service</li>
                            <li>Is illegal or promotes illegal activity</li>
                        </ul>
                        <p className="mt-4">
                            You may also terminate your account at any time by using the account deletion feature 
                            in Settings or by contacting us.
                        </p>
                    </TermsSection>

                    <TermsSection number="8" title="Disclaimer of Warranties">
                        <div className="bg-slate-100 rounded-xl p-4">
                            <p className="text-slate-700 text-sm">
                                THE SERVICE IS PROVIDED "AS IS" AND "AS AVAILABLE" WITHOUT WARRANTIES OF ANY KIND, 
                                EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO IMPLIED WARRANTIES OF 
                                MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, AND NON-INFRINGEMENT. WE DO NOT 
                                WARRANT THAT THE SERVICE WILL BE UNINTERRUPTED, SECURE, OR ERROR-FREE.
                            </p>
                        </div>
                    </TermsSection>

                    <TermsSection number="9" title="Limitation of Liability">
                        <p>
                            TO THE MAXIMUM EXTENT PERMITTED BY LAW, OPN.ONL AND ITS OPERATORS SHALL NOT BE LIABLE 
                            FOR ANY INDIRECT, INCIDENTAL, SPECIAL, CONSEQUENTIAL, OR PUNITIVE DAMAGES, OR ANY LOSS 
                            OF PROFITS OR REVENUES, WHETHER INCURRED DIRECTLY OR INDIRECTLY, OR ANY LOSS OF DATA, 
                            USE, GOODWILL, OR OTHER INTANGIBLE LOSSES.
                        </p>
                    </TermsSection>

                    <TermsSection number="10" title="Third-Party Content Disclaimer">
                        <div className="bg-red-50 border border-red-200 rounded-xl p-4 mb-4">
                            <div className="flex items-start gap-3">
                                <AlertTriangle className="h-5 w-5 text-red-600 flex-shrink-0 mt-0.5" />
                                <p className="text-red-800 text-sm">
                                    <strong>Important Notice:</strong> opn.onl is not responsible for the content 
                                    of links created by users.
                                </p>
                            </div>
                        </div>
                        <ul className="list-disc pl-5 space-y-2">
                            <li>We do not control, endorse, or verify the content accessible through shortened links</li>
                            <li>Users are solely responsible for the content they link to</li>
                            <li>We actively work to remove malicious links when reported, but cannot guarantee detection of all harmful content</li>
                            <li>If you encounter a malicious link, please report it to <a href="mailto:abuse@opn.onl" className="text-primary-600 hover:underline">abuse@opn.onl</a></li>
                            <li>We reserve the right to disable any link that violates our policies without notice</li>
                            <li>We maintain blocklists of known malicious URLs and domains to protect users</li>
                        </ul>
                    </TermsSection>

                    <TermsSection number="11" title="Changes to Terms">
                        <p>
                            We may modify these terms at any time. We will notify users of significant changes via 
                            email or a prominent notice on the website. Continued use of the service after changes 
                            constitutes acceptance of the new terms.
                        </p>
                    </TermsSection>

                    <TermsSection number="12" title="Contact">
                        <p>
                            If you have questions about these Terms of Service, please contact us at:
                        </p>
                        <a 
                            href="mailto:legal@opn.onl" 
                            className="inline-block mt-2 text-primary-600 font-medium hover:underline"
                        >
                            legal@opn.onl
                        </a>
                    </TermsSection>

                    {/* Footer */}
                    <div className="border-t border-slate-200 pt-8">
                        <p className="text-sm text-slate-500 text-center">
                            These Terms of Service are effective as of December 7, 2025.
                        </p>
                    </div>
                </motion.div>
            </section>
        </div>
    );
}

function TermsSection({ 
    number, 
    title, 
    children 
}: { 
    number: string; 
    title: string; 
    children: React.ReactNode;
}) {
    return (
        <div>
            <h3 className="text-xl font-bold text-slate-900 mb-4">
                <span className="text-primary-600">{number}.</span> {title}
            </h3>
            <div className="text-slate-600 leading-relaxed">
                {children}
            </div>
        </div>
    );
}
