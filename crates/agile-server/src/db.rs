use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Core tables
    sqlx::query(
        r#"
        -- Users
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            email VARCHAR(255) UNIQUE NOT NULL,
            name VARCHAR(255) NOT NULL,
            password_hash VARCHAR(255) NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Teams
        CREATE TABLE IF NOT EXISTS teams (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name VARCHAR(255) NOT NULL,
            owner_id UUID NOT NULL REFERENCES users(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE TABLE IF NOT EXISTS team_members (
            team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            role VARCHAR(50) NOT NULL DEFAULT 'member',
            joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (team_id, user_id)
        );

        -- Refresh tokens
        CREATE TABLE IF NOT EXISTS refresh_tokens (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            token_hash VARCHAR(255) NOT NULL,
            expires_at TIMESTAMPTZ NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user ON refresh_tokens(user_id);
        "#,
    )
    .execute(pool)
    .await?;

    // Entity-specific tables with searchable columns
    sqlx::query(
        r#"
        -- Standups (with full-text search)
        CREATE TABLE IF NOT EXISTS standups (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            external_id VARCHAR(100) NOT NULL,
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            author VARCHAR(255) NOT NULL,
            standup_date DATE NOT NULL,
            yesterday TEXT[] NOT NULL DEFAULT '{}',
            today TEXT[] NOT NULL DEFAULT '{}',
            blockers TEXT[] NOT NULL DEFAULT '{}',
            search_vector TSVECTOR,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_standups_team_date ON standups(team_id, standup_date DESC);
        CREATE INDEX IF NOT EXISTS idx_standups_author ON standups(team_id, author);
        CREATE INDEX IF NOT EXISTS idx_standups_search ON standups USING GIN(search_vector);

        -- Bugs (with priority, status, full-text search)
        CREATE TABLE IF NOT EXISTS bugs (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            external_id VARCHAR(100) NOT NULL,
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            title VARCHAR(500) NOT NULL,
            description TEXT,
            priority VARCHAR(50) NOT NULL DEFAULT 'medium',
            status VARCHAR(50) NOT NULL DEFAULT 'open',
            reporter VARCHAR(255) NOT NULL,
            assignee VARCHAR(255),
            labels TEXT[] NOT NULL DEFAULT '{}',
            search_vector TSVECTOR,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_bugs_team_status ON bugs(team_id, status);
        CREATE INDEX IF NOT EXISTS idx_bugs_team_priority ON bugs(team_id, priority);
        CREATE INDEX IF NOT EXISTS idx_bugs_assignee ON bugs(team_id, assignee);
        CREATE INDEX IF NOT EXISTS idx_bugs_labels ON bugs USING GIN(labels);
        CREATE INDEX IF NOT EXISTS idx_bugs_search ON bugs USING GIN(search_vector);

        -- Tasks (kanban board)
        CREATE TABLE IF NOT EXISTS tasks (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            external_id VARCHAR(100) NOT NULL,
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            title VARCHAR(500) NOT NULL,
            description TEXT,
            status VARCHAR(50) NOT NULL DEFAULT 'todo',
            assignee VARCHAR(255),
            labels TEXT[] NOT NULL DEFAULT '{}',
            search_vector TSVECTOR,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_tasks_team_status ON tasks(team_id, status);
        CREATE INDEX IF NOT EXISTS idx_tasks_assignee ON tasks(team_id, assignee);
        CREATE INDEX IF NOT EXISTS idx_tasks_search ON tasks USING GIN(search_vector);

        -- Retros
        CREATE TABLE IF NOT EXISTS retros (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            external_id VARCHAR(100) NOT NULL,
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            category VARCHAR(50) NOT NULL,
            content TEXT NOT NULL,
            author VARCHAR(255) NOT NULL,
            sprint VARCHAR(100),
            action_status VARCHAR(50),
            action_owner VARCHAR(255),
            search_vector TSVECTOR,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_retros_team_category ON retros(team_id, category);
        CREATE INDEX IF NOT EXISTS idx_retros_sprint ON retros(team_id, sprint);
        CREATE INDEX IF NOT EXISTS idx_retros_search ON retros USING GIN(search_vector);

        -- Decisions (ADRs)
        CREATE TABLE IF NOT EXISTS decisions (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            external_id VARCHAR(100) NOT NULL,
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            title VARCHAR(500) NOT NULL,
            context TEXT,
            decision TEXT NOT NULL,
            consequences TEXT[] NOT NULL DEFAULT '{}',
            status VARCHAR(50) NOT NULL DEFAULT 'proposed',
            author VARCHAR(255) NOT NULL,
            superseded_by VARCHAR(100),
            search_vector TSVECTOR,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_decisions_team_status ON decisions(team_id, status);
        CREATE INDEX IF NOT EXISTS idx_decisions_search ON decisions USING GIN(search_vector);

        -- Notes
        CREATE TABLE IF NOT EXISTS notes (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            external_id VARCHAR(100) NOT NULL,
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            title VARCHAR(500) NOT NULL,
            content TEXT NOT NULL,
            author VARCHAR(255) NOT NULL,
            tags TEXT[] NOT NULL DEFAULT '{}',
            search_vector TSVECTOR,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_notes_tags ON notes USING GIN(tags);
        CREATE INDEX IF NOT EXISTS idx_notes_search ON notes USING GIN(search_vector);

        -- Reviews
        CREATE TABLE IF NOT EXISTS reviews (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            external_id VARCHAR(100) NOT NULL,
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            branch VARCHAR(255) NOT NULL,
            title VARCHAR(500) NOT NULL,
            description TEXT,
            author VARCHAR(255) NOT NULL,
            reviewers TEXT[] NOT NULL DEFAULT '{}',
            status VARCHAR(50) NOT NULL DEFAULT 'pending',
            comments JSONB NOT NULL DEFAULT '[]',
            search_vector TSVECTOR,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_reviews_team_status ON reviews(team_id, status);
        CREATE INDEX IF NOT EXISTS idx_reviews_branch ON reviews(team_id, branch);
        CREATE INDEX IF NOT EXISTS idx_reviews_search ON reviews USING GIN(search_vector);

        -- Kudos
        CREATE TABLE IF NOT EXISTS kudos (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            external_id VARCHAR(100) NOT NULL,
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            from_user VARCHAR(255) NOT NULL,
            to_user VARCHAR(255) NOT NULL,
            message TEXT NOT NULL,
            category VARCHAR(100),
            search_vector TSVECTOR,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, external_id)
        );
        CREATE INDEX IF NOT EXISTS idx_kudos_to_user ON kudos(team_id, to_user);
        CREATE INDEX IF NOT EXISTS idx_kudos_from_user ON kudos(team_id, from_user);
        CREATE INDEX IF NOT EXISTS idx_kudos_search ON kudos USING GIN(search_vector);

        -- Legacy sync_data table for backward compatibility
        CREATE TABLE IF NOT EXISTS sync_data (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id),
            entity_type VARCHAR(100) NOT NULL,
            entity_id VARCHAR(100) NOT NULL,
            data JSONB NOT NULL,
            version BIGINT NOT NULL DEFAULT 1,
            deleted BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (team_id, entity_type, entity_id)
        );
        CREATE INDEX IF NOT EXISTS idx_sync_data_team_type ON sync_data(team_id, entity_type);
        CREATE INDEX IF NOT EXISTS idx_sync_data_version ON sync_data(team_id, entity_type, version);
        "#,
    )
    .execute(pool)
    .await?;

    // Create search vector update triggers
    sqlx::query(
        r#"
        -- Function to update search vectors
        CREATE OR REPLACE FUNCTION update_search_vector() RETURNS TRIGGER AS $$
        BEGIN
            CASE TG_TABLE_NAME
                WHEN 'standups' THEN
                    NEW.search_vector := to_tsvector('english',
                        coalesce(NEW.author, '') || ' ' ||
                        coalesce(array_to_string(NEW.yesterday, ' '), '') || ' ' ||
                        coalesce(array_to_string(NEW.today, ' '), '') || ' ' ||
                        coalesce(array_to_string(NEW.blockers, ' '), '')
                    );
                WHEN 'bugs' THEN
                    NEW.search_vector := to_tsvector('english',
                        coalesce(NEW.title, '') || ' ' ||
                        coalesce(NEW.description, '') || ' ' ||
                        coalesce(NEW.reporter, '') || ' ' ||
                        coalesce(NEW.assignee, '') || ' ' ||
                        coalesce(array_to_string(NEW.labels, ' '), '')
                    );
                WHEN 'tasks' THEN
                    NEW.search_vector := to_tsvector('english',
                        coalesce(NEW.title, '') || ' ' ||
                        coalesce(NEW.description, '') || ' ' ||
                        coalesce(NEW.assignee, '') || ' ' ||
                        coalesce(array_to_string(NEW.labels, ' '), '')
                    );
                WHEN 'retros' THEN
                    NEW.search_vector := to_tsvector('english',
                        coalesce(NEW.content, '') || ' ' ||
                        coalesce(NEW.author, '') || ' ' ||
                        coalesce(NEW.sprint, '')
                    );
                WHEN 'decisions' THEN
                    NEW.search_vector := to_tsvector('english',
                        coalesce(NEW.title, '') || ' ' ||
                        coalesce(NEW.context, '') || ' ' ||
                        coalesce(NEW.decision, '') || ' ' ||
                        coalesce(array_to_string(NEW.consequences, ' '), '')
                    );
                WHEN 'notes' THEN
                    NEW.search_vector := to_tsvector('english',
                        coalesce(NEW.title, '') || ' ' ||
                        coalesce(NEW.content, '') || ' ' ||
                        coalesce(array_to_string(NEW.tags, ' '), '')
                    );
                WHEN 'reviews' THEN
                    NEW.search_vector := to_tsvector('english',
                        coalesce(NEW.title, '') || ' ' ||
                        coalesce(NEW.description, '') || ' ' ||
                        coalesce(NEW.branch, '') || ' ' ||
                        coalesce(NEW.author, '')
                    );
                WHEN 'kudos' THEN
                    NEW.search_vector := to_tsvector('english',
                        coalesce(NEW.message, '') || ' ' ||
                        coalesce(NEW.from_user, '') || ' ' ||
                        coalesce(NEW.to_user, '') || ' ' ||
                        coalesce(NEW.category, '')
                    );
            END CASE;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;

        -- Create triggers for each table
        DROP TRIGGER IF EXISTS standups_search_trigger ON standups;
        CREATE TRIGGER standups_search_trigger BEFORE INSERT OR UPDATE ON standups
            FOR EACH ROW EXECUTE FUNCTION update_search_vector();

        DROP TRIGGER IF EXISTS bugs_search_trigger ON bugs;
        CREATE TRIGGER bugs_search_trigger BEFORE INSERT OR UPDATE ON bugs
            FOR EACH ROW EXECUTE FUNCTION update_search_vector();

        DROP TRIGGER IF EXISTS tasks_search_trigger ON tasks;
        CREATE TRIGGER tasks_search_trigger BEFORE INSERT OR UPDATE ON tasks
            FOR EACH ROW EXECUTE FUNCTION update_search_vector();

        DROP TRIGGER IF EXISTS retros_search_trigger ON retros;
        CREATE TRIGGER retros_search_trigger BEFORE INSERT OR UPDATE ON retros
            FOR EACH ROW EXECUTE FUNCTION update_search_vector();

        DROP TRIGGER IF EXISTS decisions_search_trigger ON decisions;
        CREATE TRIGGER decisions_search_trigger BEFORE INSERT OR UPDATE ON decisions
            FOR EACH ROW EXECUTE FUNCTION update_search_vector();

        DROP TRIGGER IF EXISTS notes_search_trigger ON notes;
        CREATE TRIGGER notes_search_trigger BEFORE INSERT OR UPDATE ON notes
            FOR EACH ROW EXECUTE FUNCTION update_search_vector();

        DROP TRIGGER IF EXISTS reviews_search_trigger ON reviews;
        CREATE TRIGGER reviews_search_trigger BEFORE INSERT OR UPDATE ON reviews
            FOR EACH ROW EXECUTE FUNCTION update_search_vector();

        DROP TRIGGER IF EXISTS kudos_search_trigger ON kudos;
        CREATE TRIGGER kudos_search_trigger BEFORE INSERT OR UPDATE ON kudos
            FOR EACH ROW EXECUTE FUNCTION update_search_vector();
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
