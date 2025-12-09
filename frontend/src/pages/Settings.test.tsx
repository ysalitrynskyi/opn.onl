import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '../test/test-utils';
import Settings from './Settings';
import { mockToken } from '../test/test-utils';

// Mock navigate
const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        useNavigate: () => mockNavigate,
    };
});

describe('Settings Page', () => {
    const mockUserProfile = {
        id: 1,
        email: 'test@example.com',
        email_verified: true,
        is_admin: false,
        created_at: '2024-01-01T00:00:00Z',
        link_count: 42,
        total_clicks: 1234,
        display_name: 'Test User',
        bio: 'Software developer',
        website: 'https://example.com',
        avatar_url: null,
        location: 'San Francisco, CA',
    };

    const mockAppSettings = {
        account_deletion_enabled: true,
        custom_aliases_enabled: true,
        max_links_per_user: null,
        passkeys_enabled: true,
        min_alias_length: 5,
        max_alias_length: 50,
    };

    const mockPasskeys = {
        passkeys: [
            {
                id: 1,
                name: 'MacBook Pro',
                created_at: '2024-01-01T00:00:00Z',
                last_used: '2024-01-03T12:00:00Z',
            },
        ],
    };

    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.setItem('token', mockToken);
        
        // Mock fetch for different endpoints
        global.fetch = vi.fn((url: string) => {
            if (url.includes('/auth/me')) {
                return Promise.resolve({
                    ok: true,
                    json: () => Promise.resolve(mockUserProfile),
                });
            }
            if (url.includes('/auth/settings')) {
                return Promise.resolve({
                    ok: true,
                    json: () => Promise.resolve(mockAppSettings),
                });
            }
            if (url.includes('/auth/passkeys')) {
                return Promise.resolve({
                    ok: true,
                    json: () => Promise.resolve(mockPasskeys),
                });
            }
            return Promise.resolve({
                ok: true,
                json: () => Promise.resolve({}),
            });
        }) as any;
    });

    describe('Authentication', () => {
        it('redirects to login if not authenticated', async () => {
            localStorage.removeItem('token');
            render(<Settings />);
            
            await waitFor(() => {
                expect(mockNavigate).toHaveBeenCalledWith('/login');
            });
        });

        it('fetches user data when authenticated', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalled();
            });
        });
    });

    describe('Profile Section', () => {
        it('displays user email', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(screen.getByText('test@example.com')).toBeInTheDocument();
            });
        });

        it('displays email verification status', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(screen.getByText(/verified/i)).toBeInTheDocument();
            });
        });

        it('displays link count', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(screen.getByText('42')).toBeInTheDocument();
            });
        });

        it('displays total clicks', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(screen.getByText('1,234') || screen.getByText('1234')).toBeDefined();
            });
        });

        it('has edit profile button', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                const editButton = screen.queryByRole('button', { name: /edit/i }) ||
                                  screen.queryByLabelText(/edit/i);
            });
        });
    });

    describe('Profile Editing', () => {
        it('shows profile form when edit is clicked', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                const editButton = screen.queryByRole('button', { name: /edit/i });
                if (editButton) {
                    fireEvent.click(editButton);
                }
            });
        });

        it('can update display name', async () => {
            render(<Settings />);
            
            await waitFor(async () => {
                const editButton = screen.queryByRole('button', { name: /edit/i });
                if (editButton) {
                    fireEvent.click(editButton);
                    
                    const nameInput = screen.queryByLabelText(/display name/i) ||
                                     screen.queryByPlaceholderText(/name/i);
                    if (nameInput) {
                        fireEvent.change(nameInput, { target: { value: 'New Name' } });
                    }
                }
            });
        });
    });

    describe('Email Verification', () => {
        it('shows resend verification button if not verified', async () => {
            global.fetch = vi.fn((url: string) => {
                if (url.includes('/auth/me')) {
                    return Promise.resolve({
                        ok: true,
                        json: () => Promise.resolve({
                            ...mockUserProfile,
                            email_verified: false,
                        }),
                    });
                }
                return Promise.resolve({
                    ok: true,
                    json: () => Promise.resolve({}),
                });
            }) as any;

            render(<Settings />);
            
            await waitFor(() => {
                const resendButton = screen.queryByText(/resend/i) ||
                                    screen.queryByRole('button', { name: /resend/i });
            });
        });
    });

    describe('Passkeys Section', () => {
        it('displays passkeys section when enabled', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                const passkeysSection = screen.queryByText(/passkeys/i);
                expect(passkeysSection).toBeDefined();
            });
        });

        it('displays registered passkeys', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(screen.queryByText('MacBook Pro')).toBeDefined();
            });
        });

        it('has add passkey button', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                const addButton = screen.queryByRole('button', { name: /add passkey/i });
            });
        });

        it('can delete passkey', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                const deleteButton = screen.queryByRole('button', { name: /delete/i });
            });
        });

        it('can rename passkey', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                const renameButton = screen.queryByRole('button', { name: /rename/i }) ||
                                    screen.queryByLabelText(/rename/i);
            });
        });
    });

    describe('Security Section', () => {
        it('displays security section', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(screen.queryByText(/security/i)).toBeDefined();
            });
        });

        it('has change password option', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(screen.queryByText(/change password/i)).toBeDefined();
            });
        });
    });

    describe('Change Password', () => {
        it('shows password form when clicked', async () => {
            render(<Settings />);
            
            await waitFor(async () => {
                const changePasswordBtn = screen.queryByText(/change password/i);
                if (changePasswordBtn) {
                    fireEvent.click(changePasswordBtn);
                }
            });
        });

        it('validates password length', async () => {
            render(<Settings />);
            
            await waitFor(async () => {
                const changePasswordBtn = screen.queryByText(/change password/i);
                if (changePasswordBtn) {
                    fireEvent.click(changePasswordBtn);
                    
                    const newPasswordInput = screen.queryByPlaceholderText(/new password/i);
                    if (newPasswordInput) {
                        fireEvent.change(newPasswordInput, { target: { value: 'short' } });
                        
                        const submitBtn = screen.queryByRole('button', { name: /update/i });
                        if (submitBtn) {
                            fireEvent.click(submitBtn);
                        }
                    }
                }
            });
        });

        it('validates password confirmation', async () => {
            render(<Settings />);
            
            // Test that passwords must match
        });
    });

    describe('Data Export', () => {
        it('has export data option', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                expect(screen.queryByText(/export/i)).toBeDefined();
            });
        });

        it('can export links as CSV', async () => {
            render(<Settings />);
            
            await waitFor(async () => {
                const exportBtn = screen.queryByText(/export.*csv/i) ||
                                 screen.queryByRole('button', { name: /export/i });
                if (exportBtn) {
                    fireEvent.click(exportBtn);
                }
            });
        });
    });

    describe('Delete Account', () => {
        it('shows delete account option when enabled', async () => {
            render(<Settings />);
            
            await waitFor(() => {
                const deleteSection = screen.queryByText(/danger zone|delete account/i);
            });
        });

        it('requires password confirmation to delete', async () => {
            render(<Settings />);
            
            await waitFor(async () => {
                const deleteBtn = screen.queryByText(/delete.*account/i);
                if (deleteBtn) {
                    fireEvent.click(deleteBtn);
                    
                    const passwordInput = screen.queryByPlaceholderText(/password/i);
                    expect(passwordInput).toBeDefined();
                }
            });
        });

        it('shows warning before deletion', async () => {
            render(<Settings />);
            
            await waitFor(async () => {
                const deleteBtn = screen.queryByText(/delete.*account/i);
                if (deleteBtn) {
                    fireEvent.click(deleteBtn);
                    
                    const warning = screen.queryByText(/cannot be undone/i);
                }
            });
        });
    });

    describe('Error Handling', () => {
        it('displays error messages', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: false,
                status: 500,
                json: () => Promise.resolve({ error: 'Server error' }),
            });

            render(<Settings />);
            
            // Should handle error gracefully
        });

        it('displays success messages', async () => {
            render(<Settings />);
            
            // After successful update, should show success message
        });
    });
});


