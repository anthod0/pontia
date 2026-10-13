<script lang="ts">
  import LegalPage from "$lib/components/LegalPage.svelte";
</script>

<LegalPage
  title="Terms of Service"
  description="The terms that apply when you use Pontia's website and hosted services."
>
  <h1>Terms of Service</h1>
  <p class="effective-date">Effective October 13, 2026</p>

  <p>
    These Terms of Service ("Terms") govern your access to and use of pontia.dev, Pontia Cloud, and
    the hosted Remote Dashboard (collectively, the "Hosted Services"). Pontia ("Pontia," "we," "us,"
    or "our") operates the Hosted Services. By using the Hosted Services, you agree to these Terms.
    If you do not agree, do not use the Hosted Services.
  </p>

  <h2>1. Pontia software and Hosted Services</h2>
  <p>
    Pontia is local-first software for running and controlling coding agents on your own devices. The
    Hosted Services provide identity, device registration, connection authorization, and access to a
    remote Dashboard. They do not provide your coding agent, model-provider account, development
    environment, or device administration.
  </p>
  <p>
    Pontia's open-source code is licensed separately under the license included with that code. These
    Terms govern the Hosted Services and do not replace or limit rights granted by an open-source
    license. Infrastructure you self-host is your responsibility. Contributing to Pontia does not, by
    itself, make a contributor an operator of the Hosted Services or a party to these Terms.
  </p>

  <h2>2. Eligibility and authority</h2>
  <p>
    You must be legally able to enter into these Terms. If you use the Hosted Services for an
    organization, you represent that you have authority to bind that organization, and "you" includes
    the organization.
  </p>

  <h2>3. Accounts</h2>
  <p>
    You sign in through a supported third-party identity provider. You are responsible for the
    accuracy of your account information, for maintaining control of your sign-in account and devices,
    and for all activity authorized through your credentials. Promptly revoke exposed sessions and
    credentials and notify us if you believe your account or a Pontia service has been compromised.
  </p>
  <p>
    You may not share credentials in a way that bypasses account or authorization controls, impersonate
    another person, or create accounts through deceptive or automated means.
  </p>

  <h2>4. Your devices, agents, and content</h2>
  <p>
    You retain your rights in source code, prompts, agent output, project files, and other content you
    control ("Your Content"). You are responsible for Your Content, the actions you ask agents to take,
    your backups, and the permissions granted to Pontia, Agent Clients, model providers, tools, and
    users of your devices.
  </p>
  <p>
    You grant us only the limited permission needed to operate the Hosted Services at your direction,
    such as maintaining account and device registrations and authorizing connections. Pontia Cloud does
    not store your project files or plaintext agent-session content. Remote Dashboard traffic is
    encrypted end to end between your browser and your device, as described in the
    <a href="/docs/security">Security documentation</a>.
  </p>

  <h2>5. Acceptable use</h2>
  <p>You must not use the Hosted Services to:</p>
  <ul>
    <li>violate law or the rights of another person;</li>
    <li>access devices, accounts, systems, or data without authorization;</li>
    <li>distribute malware or facilitate phishing, fraud, exploitation, or credential theft;</li>
    <li>interfere with, overload, probe, or circumvent the security or rate limits of the service;</li>
    <li>use another user's device handle, Edge, credentials, or account without permission;</li>
    <li>resell or provide the Hosted Services to third parties without our permission; or</li>
    <li>help another person do any of the above.</li>
  </ul>
  <p>
    Security research must avoid privacy violations, data destruction, service disruption, and access
    to another user's data. Contact us before testing that could affect the Hosted Services or their
    users.
  </p>

  <h2>6. Third-party services</h2>
  <p>
    Pontia can interact with services not controlled by us, including Google, GitHub, coding-agent
    clients, model providers, and tools selected by you. Your use of those services is governed by
    their own terms and policies. We are not responsible for third-party services, their availability,
    or the content they produce.
  </p>

  <h2>7. Service changes and availability</h2>
  <p>
    Pontia is a community-maintained project under active development, and the Hosted Services are
    offered without a usage fee. We may add, change, limit, or discontinue features, perform
    maintenance, or stop operating some or all of the Hosted Services without notice. We aim to
    operate them reliably but do not promise uninterrupted, secure, or error-free availability. You
    should maintain backups and a way to access and manage your devices without relying solely on the
    Hosted Services.
  </p>

  <h2>8. Suspension and termination</h2>
  <p>
    You may stop using the Hosted Services at any time. You can revoke sessions and remove device or
    Edge registrations in account settings, and you may contact us to request account deletion.
  </p>
  <p>
    We may suspend or terminate access when reasonably necessary to address a violation of these Terms,
    security risk, legal requirement, harm to another user, or material risk to the Hosted Services.
    Where practical, we will provide notice and an opportunity to resolve the issue. Provisions that by
    their nature should survive termination—including ownership, disclaimers, and limitations of
    liability—will survive.
  </p>

  <h2>9. Feedback</h2>
  <p>
    If you provide feedback or suggestions, you grant us permission to use them without restriction or
    compensation. This does not transfer ownership of Your Content.
  </p>

  <h2>10. Disclaimers</h2>
  <p>
    TO THE MAXIMUM EXTENT PERMITTED BY LAW, THE HOSTED SERVICES ARE PROVIDED "AS IS" AND "AS
    AVAILABLE." WE DISCLAIM ALL WARRANTIES, EXPRESS OR IMPLIED, INCLUDING WARRANTIES OF
    MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, TITLE, AND NON-INFRINGEMENT. WE DO NOT WARRANT
    THAT AGENT OUTPUT IS ACCURATE, SAFE, OR SUITABLE FOR YOUR PURPOSES. YOU ARE RESPONSIBLE FOR
    REVIEWING COMMANDS, CODE CHANGES, AND OUTPUT BEFORE RELYING ON THEM.
  </p>
  <p>
    Nothing in these Terms excludes a warranty or consumer right that applicable law does not allow us
    to exclude.
  </p>

  <h2>11. Limitation of liability</h2>
  <p>
    TO THE MAXIMUM EXTENT PERMITTED BY LAW, PONTIA WILL NOT BE LIABLE FOR INDIRECT,
    INCIDENTAL, SPECIAL, CONSEQUENTIAL, EXEMPLARY, OR PUNITIVE DAMAGES, OR FOR LOST PROFITS, REVENUE,
    DATA, GOODWILL, OR BUSINESS INTERRUPTION, ARISING FROM OR RELATED TO THE HOSTED SERVICES, EVEN IF
    ADVISED THAT SUCH DAMAGES ARE POSSIBLE.
  </p>
  <p>
    TO THE MAXIMUM EXTENT PERMITTED BY LAW, OUR TOTAL LIABILITY FOR ALL CLAIMS ARISING FROM OR RELATED
    TO THE HOSTED SERVICES WILL NOT EXCEED THE AMOUNT YOU PAID US FOR THE HOSTED SERVICES DURING THE
    TWELVE MONTHS BEFORE THE EVENT GIVING RISE TO THE CLAIM. These limits do not apply where prohibited
    by law.
  </p>

  <h2>12. Changes to these Terms</h2>
  <p>
    We may update these Terms to reflect changes to Pontia, law, or our operating practices. We will
    post the updated Terms here and change the effective date. For material changes, we will provide
    additional notice when reasonably possible. Your continued use after updated Terms take effect
    means you accept them; if you do not agree, you must stop using the Hosted Services.
  </p>

  <h2>13. General</h2>
  <p>
    These Terms and the <a href="/privacy">Privacy Policy</a> describe the agreement between you and
    Pontia regarding the Hosted Services. They do not alter the license for Pontia's
    open-source code. If any provision is unenforceable, the remaining provisions stay in effect. Our
    failure to enforce a provision is not a waiver.
  </p>

  <h2>14. Contact</h2>
  <p>
    Questions about these Terms may be directed to Pontia at
    <a href="mailto:contact@pontia.dev">contact@pontia.dev</a>.
  </p>
</LegalPage>
