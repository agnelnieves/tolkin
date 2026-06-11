# Self-authored fixture for the lossy/repomix comparison row.
# See auth.ts header for provenance.
"""Send transactional emails via the configured provider."""
import logging
import os
from dataclasses import dataclass
from typing import Optional

logger = logging.getLogger(__name__)


@dataclass
class EmailMessage:
    to: str
    subject: str
    body: str
    reply_to: Optional[str] = None


class EmailService:
    """Wraps the underlying SMTP or API client and exposes a stable send method."""

    def __init__(self, provider: str, api_key: str) -> None:
        self.provider = provider
        self.api_key = api_key
        self._client = self._build_client()

    def _build_client(self):
        if self.provider == "resend":
            from resend import Client
            return Client(api_key=self.api_key)
        if self.provider == "smtp":
            from smtplib import SMTP
            host = os.environ.get("SMTP_HOST", "localhost")
            port = int(os.environ.get("SMTP_PORT", "587"))
            return SMTP(host, port)
        raise ValueError(f"unknown email provider: {self.provider}")

    def send(self, message: EmailMessage) -> str:
        """Send the message; returns the provider's message id."""
        if self.provider == "resend":
            resp = self._client.emails.send(
                {
                    "from": "noreply@example.com",
                    "to": message.to,
                    "subject": message.subject,
                    "html": message.body,
                    "reply_to": message.reply_to,
                }
            )
            logger.info("sent message %s to %s", resp.id, message.to)
            return resp.id
        if self.provider == "smtp":
            msg_id = self._client.send_message(self._to_mime(message))
            logger.info("sent smtp message %s to %s", msg_id, message.to)
            return msg_id
        raise ValueError(f"unknown email provider: {self.provider}")

    def _to_mime(self, message: EmailMessage):
        from email.mime.text import MIMEText
        mime = MIMEText(message.body, "html")
        mime["Subject"] = message.subject
        mime["To"] = message.to
        if message.reply_to:
            mime["Reply-To"] = message.reply_to
        return mime


def render_welcome_email(name: str, login_link: str) -> EmailMessage:
    """Render the standard welcome email template."""
    body = f"""
    <html>
      <body>
        <p>Hi {name},</p>
        <p>Welcome to the service. You can finish signing in here:
          <a href="{login_link}">{login_link}</a>
        </p>
        <p>The link expires in 24 hours. If you did not request an account,
          you can safely ignore this message.</p>
      </body>
    </html>
    """
    return EmailMessage(
        to=name,
        subject="Welcome to the service",
        body=body,
    )
